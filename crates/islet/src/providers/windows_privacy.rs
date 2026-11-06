//! Camera and microphone in-use dots on Windows, event-driven.
//!
//! Windows records each app's use of the camera and microphone under
//! `HKCU\…\CapabilityAccessManager\ConsentStore\{webcam,microphone}`: an
//! app is using the device while its `LastUsedTimeStop` is 0. (This is the
//! same store Settings → Privacy shows "recent activity" from.) The thread
//! sleeps in `WaitForMultipleObjects` until the registry changes.

use super::Sink;
use islet_core::activity::{Privacy, Report};
use std::collections::BTreeSet;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegNotifyChangeKeyValue, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET, REG_NOTIFY_CHANGE_NAME,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForMultipleObjects, INFINITE};

const STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Key(HKEY);

impl Key {
    fn open(parent: HKEY, path: &str, access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<Key> {
        let w = wide(path);
        let mut k = HKEY::default();
        // SAFETY: valid NUL-terminated path and out pointer.
        let r = unsafe { RegOpenKeyExW(parent, PCWSTR(w.as_ptr()), None, access, &mut k) };
        (r == ERROR_SUCCESS).then_some(Key(k))
    }

    fn subkeys(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut buf = [0u16; 512];
        for i in 0.. {
            let mut len = buf.len() as u32;
            // SAFETY: buffer and length describe the same array.
            let r =
                unsafe { RegEnumKeyExW(self.0, i, Some(PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None) };
            if r != ERROR_SUCCESS {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        out
    }

    fn qword(&self, name: &str) -> Option<u64> {
        let w = wide(name);
        let mut v = 0u64;
        let mut len = 8u32;
        // SAFETY: an 8-byte buffer for a REG_QWORD.
        let r = unsafe {
            RegQueryValueExW(self.0, PCWSTR(w.as_ptr()), None, None, Some((&mut v as *mut u64).cast()), Some(&mut len))
        };
        (r == ERROR_SUCCESS && len == 8).then_some(v)
    }

    /// The app is using the device right now.
    fn in_use(&self) -> bool {
        self.qword("LastUsedTimeStart").is_some_and(|s| s != 0) && self.qword("LastUsedTimeStop") == Some(0)
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the key was opened by Key::open.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

/// Apps using one capability ("webcam" or "microphone"), or None.
fn users(cap: &Key) -> Option<Vec<String>> {
    let mut names = BTreeSet::new();
    for sub in cap.subkeys() {
        let Some(k) = Key::open(cap.0, &sub, KEY_READ) else { continue };
        if sub.eq_ignore_ascii_case("NonPackaged") {
            for exe in k.subkeys() {
                if Key::open(k.0, &exe, KEY_READ).is_some_and(|e| e.in_use()) {
                    names.insert(super::consent_app_name(&exe));
                }
            }
        } else if k.in_use() {
            names.insert(super::consent_app_name(&sub));
        }
    }
    (!names.is_empty()).then(|| names.into_iter().collect())
}

pub fn start(sink: Sink) {
    let _ = std::thread::Builder::new().name("privacy".into()).spawn(move || run(sink));
}

fn run(sink: Sink) {
    let caps: Vec<Key> = ["webcam", "microphone"]
        .iter()
        .filter_map(|c| Key::open(HKEY_CURRENT_USER, &format!("{STORE}{c}"), KEY_READ | KEY_NOTIFY))
        .collect();
    if caps.len() != 2 {
        return; // older Windows without the consent store
    }
    // SAFETY: unnamed auto-reset events, owned by this thread for its life.
    let events: Vec<HANDLE> = match (0..2).map(|_| unsafe { CreateEventW(None, false, false, None) }).collect() {
        Ok(v) => v,
        Err(_) => return,
    };
    let arm = |i: usize| {
        // SAFETY: open key and live event handle; asynchronous notification.
        unsafe {
            RegNotifyChangeKeyValue(
                caps[i].0,
                true,
                REG_NOTIFY_CHANGE_LAST_SET | REG_NOTIFY_CHANGE_NAME,
                Some(events[i]),
                true,
            )
        }
    };
    let mut state = Privacy::default();
    // Arm before reading, so a change in between still wakes us. Each
    // registration fires once; only the one that fired is armed again.
    if arm(0) != ERROR_SUCCESS || arm(1) != ERROR_SUCCESS {
        return;
    }
    loop {
        let next = Privacy { camera: users(&caps[0]), mic: users(&caps[1]) };
        if next != state {
            state = next;
            sink.send(Report::Privacy(state.clone()));
        }
        // SAFETY: both handles stay valid for the loop's life.
        let r = unsafe { WaitForMultipleObjects(&events, false, INFINITE) };
        let i = r.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
        if i >= events.len() || arm(i) != ERROR_SUCCESS {
            return;
        }
    }
}
