//! Other apps' notifications on Windows (opt-in), through
//! UserNotificationListener. Windows grants it only to packaged apps, so
//! this works in the MSIX build and says so otherwise. The user also
//! allows it once in Settings → Privacy → Notifications.
//!
//! Windows announces new toasts with an event where it can; where it
//! can't (desktop apps on some Windows builds), Islet checks for new ones
//! every 2 seconds, and only while notifications are turned on.

use super::{debug_log, Sink};
use islet_core::activity::{Note, Report};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::Management::{UserNotificationListener, UserNotificationListenerAccessStatus};
use windows::UI::Notifications::{
    KnownNotificationBindings, NotificationKinds, UserNotification, UserNotificationChangedEventArgs,
    UserNotificationChangedKind,
};

const POLL: Duration = Duration::from_secs(2);

/// A running listener; dropping it stops it.
pub struct Listener {
    stop: Arc<AtomicBool>,
    listener: Option<(UserNotificationListener, i64)>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some((l, token)) = self.listener.take() {
            let _ = l.RemoveNotificationChanged(token);
        }
    }
}

fn note_of(n: &UserNotification) -> Option<Note> {
    let app = n
        .AppInfo()
        .and_then(|a| a.DisplayInfo())
        .and_then(|d| d.DisplayName())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let binding =
        n.Notification().ok()?.Visual().ok()?.GetBinding(&KnownNotificationBindings::ToastGeneric().ok()?).ok()?;
    let texts: Vec<String> =
        binding.GetTextElements().ok()?.into_iter().filter_map(|t| t.Text().ok().map(|s| s.to_string())).collect();
    let summary = texts.first().cloned().unwrap_or_default();
    if summary.is_empty() && app.is_empty() {
        return None;
    }
    Some(Note { app, summary, body: texts.get(1..).map(|r| r.join("\n")).unwrap_or_default() })
}

pub fn start(sink: Sink) -> Result<Listener, String> {
    if !crate::msix::packaged() {
        return Err("Windows lets only the MSIX build of Islet read notifications".into());
    }
    let listener = UserNotificationListener::Current().map_err(|e| e.message().to_string())?;
    let access = listener.RequestAccessAsync().and_then(|a| a.join()).map_err(|e| e.message().to_string())?;
    if access != UserNotificationListenerAccessStatus::Allowed {
        return Err("allow Islet in Settings → Privacy & security → Notifications".into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let (l2, s2) = (listener.clone(), sink.clone());
    let handler =
        TypedEventHandler::<UserNotificationListener, UserNotificationChangedEventArgs>::new(move |_, args| {
            if let Some(a) = args.as_ref() {
                if a.ChangeKind()? == UserNotificationChangedKind::Added {
                    if let Some(note) = l2.GetNotification(a.UserNotificationId()?).ok().as_ref().and_then(note_of) {
                        s2.send(Report::Notification(note));
                    }
                }
            }
            Ok(())
        });
    match listener.NotificationChanged(&handler) {
        Ok(token) => Ok(Listener { stop, listener: Some((listener, token)) }),
        Err(e) => {
            debug_log(&format!("notifications: no change event ({}); checking every 2 s", e.message()));
            let st = stop.clone();
            std::thread::Builder::new()
                .name("notifications".into())
                .spawn(move || poll(&listener, &sink, &st))
                .map_err(|e| e.to_string())?;
            Ok(Listener { stop, listener: None })
        }
    }
}

fn poll(listener: &UserNotificationListener, sink: &Sink, stop: &AtomicBool) {
    let ids = |l: &UserNotificationListener| -> Vec<UserNotification> {
        l.GetNotificationsAsync(NotificationKinds::Toast)
            .and_then(|a| a.join())
            .map(|v| v.into_iter().collect())
            .unwrap_or_default()
    };
    // What's already in the Action Center isn't news.
    let mut seen: HashSet<u32> = ids(listener).iter().filter_map(|n| n.Id().ok()).collect();
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(POLL);
        let now = ids(listener);
        let current: HashSet<u32> = now.iter().filter_map(|n| n.Id().ok()).collect();
        for n in &now {
            if n.Id().is_ok_and(|id| !seen.contains(&id)) {
                if let Some(note) = note_of(n) {
                    sink.send(Report::Notification(note));
                }
            }
        }
        seen = current;
    }
}
