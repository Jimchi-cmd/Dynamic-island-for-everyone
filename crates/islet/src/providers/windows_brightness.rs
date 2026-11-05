//! Screen brightness on Windows: WMI raises `WmiMonitorBrightnessEvent` when
//! a built-in display's brightness changes (laptops; external monitors don't
//! report it). The query blocks until an event arrives: no polling.

use super::{debug_log, Sink};
use islet_core::activity::{Hud, Report};
use windows::core::{w, BSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoInitializeSecurity, CoSetProxyBlanket, CLSCTX_INPROC_SERVER,
    COINIT_MULTITHREADED, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_AUTHN_LEVEL_DEFAULT, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, WbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE,
};

fn run(sink: Sink) -> windows::core::Result<()> {
    // SAFETY: COM/WMI calls on this thread after CoInitializeEx.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        // May already be set for the process; that's fine.
        let _ = CoInitializeSecurity(
            None,
            -1,
            None,
            None,
            RPC_C_AUTHN_LEVEL_DEFAULT,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
            None,
        );
        let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
        let services = locator.ConnectServer(
            &BSTR::from("ROOT\\WMI"),
            &BSTR::new(),
            &BSTR::new(),
            &BSTR::new(),
            0,
            &BSTR::new(),
            None,
        )?;
        CoSetProxyBlanket(
            &services,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            None,
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )?;
        let events = services.ExecNotificationQuery(
            &BSTR::from("WQL"),
            &BSTR::from("SELECT * FROM WmiMonitorBrightnessEvent"),
            WBEM_FLAG_RETURN_IMMEDIATELY | WBEM_FLAG_FORWARD_ONLY,
            None,
        )?;
        loop {
            let mut objects: [Option<IWbemClassObject>; 1] = [None];
            let mut returned = 0u32;
            events.Next(WBEM_INFINITE, &mut objects, &mut returned).ok()?;
            let Some(obj) = objects[0].take().filter(|_| returned == 1) else { continue };
            let mut value = VARIANT::default();
            if obj.Get(w!("Brightness"), 0, &mut value, None, None).is_ok() {
                // A uint8 percentage.
                let pct = value.Anonymous.Anonymous.Anonymous.bVal;
                sink.send(Report::Hud(Hud::Brightness(f32::from(pct) / 100.0)));
            }
        }
    }
}

pub fn start(sink: Sink) {
    let _ = std::thread::Builder::new().name("islet-brightness".into()).spawn(move || {
        if let Err(e) = run(sink) {
            debug_log(&format!("brightness pop-ups unavailable: {}", e.message()));
        }
    });
}
