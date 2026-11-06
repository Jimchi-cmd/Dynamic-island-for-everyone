//! Windows package identity: when Islet runs from its MSIX package it can
//! read other apps' notifications (Windows allows that only for packaged
//! apps) and starts at login through the package's startup task instead
//! of the registry (which packages see a private copy of).

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

/// The startup task's id in AppxManifest.xml.
pub const STARTUP_TASK: &str = "IsletStartup";

/// Running from the MSIX package?
pub fn packaged() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let mut len = 0u32;
        // SAFETY: a size query; the buffer is optional.
        let r = unsafe { GetCurrentPackageFullName(&mut len, None::<PWSTR>) };
        // 15700 = APPMODEL_ERROR_NO_PACKAGE; 122 = ERROR_INSUFFICIENT_BUFFER (has one).
        r.0 != 15700 && len > 0
    })
}

fn task() -> Result<windows::ApplicationModel::StartupTask, String> {
    windows::ApplicationModel::StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK))
        .and_then(|a| a.join())
        .map_err(|e| e.message().to_string())
}

pub fn set_autostart(on: bool) -> Result<(), String> {
    use windows::ApplicationModel::StartupTaskState as S;
    let t = task()?;
    if on {
        match t.RequestEnableAsync().and_then(|a| a.join()).map_err(|e| e.message().to_string())? {
            S::Enabled | S::EnabledByPolicy => Ok(()),
            S::DisabledByUser => Err("turned off in Task Manager → Startup apps; turn it on there".into()),
            S::DisabledByPolicy => Err("your organization doesn't allow it".into()),
            _ => Err("Windows didn't allow it".into()),
        }
    } else {
        t.Disable().map_err(|e| e.message().to_string())
    }
}

pub fn autostart_on() -> bool {
    use windows::ApplicationModel::StartupTaskState as S;
    task()
        .and_then(|t| t.State().map_err(|e| e.message().to_string()))
        .is_ok_and(|s| s == S::Enabled || s == S::EnabledByPolicy)
}
