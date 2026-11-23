//! Windows package identity: when Islet runs from its MSIX package it can
//! read other apps' notifications (Windows allows that only for packaged
//! apps) and starts at login through the package's startup task instead
//! of the registry (which packages see a private copy of).

use windows::core::PWSTR;
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

/// The startup tasks' ids in AppxManifest.xml. Windows may look a task up
/// under the app asking for it, and Settings (another app in the package)
/// is what turns starting at login on, so each app declares one; both
/// start the island.
pub const STARTUP_TASKS: [&str; 2] = ["IsletStartup", "IsletStartupFromSettings"];

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

/// The package's startup tasks this process can see.
fn tasks() -> Result<Vec<windows::ApplicationModel::StartupTask>, String> {
    let all = windows::ApplicationModel::StartupTask::GetForCurrentPackageAsync()
        .and_then(|a| a.join())
        .map_err(|e| e.message().to_string())?;
    let mine: Vec<_> = all
        .into_iter()
        .filter(|t| t.TaskId().is_ok_and(|id| STARTUP_TASKS.contains(&id.to_string().as_str())))
        .collect();
    if mine.is_empty() {
        return Err(NO_TASK.into());
    }
    Ok(mine)
}

const NO_TASK: &str = "Windows didn't find Islet's startup task. Turn Islet on in Task Manager → Startup apps instead.";

pub fn set_autostart(on: bool) -> Result<(), String> {
    use windows::ApplicationModel::StartupTaskState as S;
    let tasks = tasks()?;
    if !on {
        for t in &tasks {
            t.Disable().map_err(|e| e.message().to_string())?;
        }
        return Ok(());
    }
    // One is enough (two would start the island twice).
    let t = &tasks[0];
    match t.RequestEnableAsync().and_then(|a| a.join()).map_err(|e| e.message().to_string())? {
        S::Enabled | S::EnabledByPolicy => Ok(()),
        S::DisabledByUser => Err("turned off in Task Manager → Startup apps; turn it on there".into()),
        S::DisabledByPolicy => Err("your organization doesn't allow it".into()),
        _ => Err("Windows didn't allow it; turn Islet on in Task Manager → Startup apps".into()),
    }
}

pub fn autostart_on() -> bool {
    use windows::ApplicationModel::StartupTaskState as S;
    tasks().is_ok_and(|ts| ts.iter().any(|t| t.State().is_ok_and(|s| s == S::Enabled || s == S::EnabledByPolicy)))
}
