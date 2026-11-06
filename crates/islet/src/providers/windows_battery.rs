//! Windows: battery levels.
//!
//! - Bluetooth devices: a `DeviceWatcher` over Bluetooth PnP devices with
//!   the battery property Windows itself shows in Settings → Bluetooth &
//!   devices (DEVPKEY_Bluetooth_Battery). Classic headsets (hands-free
//!   battery reports) and Bluetooth LE devices (Battery Service) both
//!   surface here. Several device nodes of one physical device share a
//!   container id; they are merged.
//! - The laptop: `Battery::AggregateBattery().ReportUpdated` and
//!   `PowerManager::PowerSupplyStatusChanged` (charger plug/unplug).
//!
//! Event-driven; nothing is polled.

use super::{debug_log, kind_from_name, Sink};
use islet_core::activity::{BatteryDevice, DeviceKind, Report};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use windows::core::{Interface, HSTRING};
use windows::Devices::Enumeration::{DeviceInformation, DeviceInformationKind, DeviceWatcher};
use windows::Devices::Power::Battery;
use windows::Foundation::{EventHandler, IReference, TypedEventHandler};
use windows::System::Power::{BatteryStatus, PowerManager, PowerSupplyStatus};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows_collections::{IIterable, IMapView};

/// DEVPKEY_Bluetooth_Battery: percentage as a byte.
const BATTERY_KEY: &str = "{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2";
const CONTAINER_KEY: &str = "System.Devices.ContainerId";
/// GUID_DEVCLASS_BLUETOOTH
const BLUETOOTH_CLASS: &str = "System.Devices.ClassGuid:=\"{e0cbf06c-cd8b-4647-bb8a-263b43f0f974}\"";

/// "WH-1000XM4 Hands-Free AG" → "WH-1000XM4".
pub fn clean_name(name: &str) -> String {
    const SUFFIXES: [&str; 6] = [" Hands-Free AG", " Hands-Free", " Avrcp Transport", " Stereo", " A2DP SNK", " LE"];
    let mut n = name.trim();
    for s in SUFFIXES {
        if let Some(stripped) = n.strip_suffix(s) {
            n = stripped.trim_end();
        }
    }
    n.to_string()
}

fn battery_of(props: &IMapView<HSTRING, windows::core::IInspectable>) -> Option<u8> {
    let value = props.Lookup(&HSTRING::from(BATTERY_KEY)).ok()?;
    value.cast::<IReference<u8>>().ok()?.Value().ok()
}

fn container_of(props: &IMapView<HSTRING, windows::core::IInspectable>) -> Option<String> {
    let value = props.Lookup(&HSTRING::from(CONTAINER_KEY)).ok()?;
    value.cast::<IReference<windows::core::GUID>>().ok()?.Value().ok().map(|g| format!("{g:?}"))
}

#[derive(Default)]
struct Devices {
    /// device id → (container id, name)
    nodes: HashMap<String, (String, String)>,
    /// container id → last reported level
    levels: HashMap<String, u8>,
    enumerated: bool,
}

fn device(container: &str, name: &str, percent: u8) -> BatteryDevice {
    BatteryDevice {
        id: format!("win:{container}"),
        name: clean_name(name),
        kind: kind_from_name(name),
        percent: percent.min(100),
        charging: false,
        parts: vec![],
    }
}

fn watch_bluetooth(sink: Sink) -> windows::core::Result<DeviceWatcher> {
    let props: IIterable<HSTRING> = vec![HSTRING::from(BATTERY_KEY), HSTRING::from(CONTAINER_KEY)].into();
    let watcher = DeviceInformation::CreateWatcherWithKindAqsFilterAndAdditionalProperties(
        &HSTRING::from(BLUETOOTH_CLASS),
        &props,
        DeviceInformationKind::Device,
    )?;
    let state = Arc::new(Mutex::new(Devices::default()));

    {
        let (state, sink) = (state.clone(), sink.clone());
        watcher.Added(&TypedEventHandler::<DeviceWatcher, DeviceInformation>::new(move |_, info| {
            let Some(info) = info.as_ref() else { return Ok(()) };
            let props = info.Properties()?;
            let id = info.Id()?.to_string();
            let name = info.Name()?.to_string();
            let container = container_of(&props).unwrap_or_else(|| id.clone());
            let mut st = state.lock().unwrap();
            st.nodes.insert(id, (container.clone(), name.clone()));
            if let Some(pct) = battery_of(&props) {
                if st.levels.insert(container.clone(), pct).is_none() {
                    let d = device(&container, &name, pct);
                    // Present at startup: record silently; afterwards: a connect peek.
                    sink.send(if st.enumerated { Report::DeviceConnected(d) } else { Report::DeviceChanged(d) });
                }
            }
            Ok(())
        }))?;
    }
    {
        let (state, sink) = (state.clone(), sink.clone());
        watcher.Updated(&TypedEventHandler::new(
            move |_, update: windows::core::Ref<windows::Devices::Enumeration::DeviceInformationUpdate>| {
                let Some(update) = update.as_ref() else { return Ok(()) };
                let id = update.Id()?.to_string();
                let props = update.Properties()?;
                let mut st = state.lock().unwrap();
                let Some((container, name)) = st.nodes.get(&id).cloned() else { return Ok(()) };
                match battery_of(&props) {
                    Some(pct) => {
                        let first = st.levels.insert(container.clone(), pct).is_none();
                        let d = device(&container, &name, pct);
                        // A level appearing means the device just connected.
                        sink.send(if first { Report::DeviceConnected(d) } else { Report::DeviceChanged(d) });
                    }
                    None => {
                        if props.HasKey(&HSTRING::from(BATTERY_KEY)).unwrap_or(false)
                            && st.levels.remove(&container).is_some()
                        {
                            sink.send(Report::DeviceGone(format!("win:{container}")));
                        }
                    }
                }
                Ok(())
            },
        ))?;
    }
    {
        let (state, sink) = (state.clone(), sink.clone());
        watcher.Removed(&TypedEventHandler::new(
            move |_, update: windows::core::Ref<windows::Devices::Enumeration::DeviceInformationUpdate>| {
                let Some(update) = update.as_ref() else { return Ok(()) };
                let id = update.Id()?.to_string();
                let mut st = state.lock().unwrap();
                if let Some((container, _)) = st.nodes.remove(&id) {
                    let still_there = st.nodes.values().any(|(c, _)| *c == container);
                    if !still_there && st.levels.remove(&container).is_some() {
                        sink.send(Report::DeviceGone(format!("win:{container}")));
                    }
                }
                Ok(())
            },
        ))?;
    }
    {
        let state = state.clone();
        watcher.EnumerationCompleted(&TypedEventHandler::new(move |_, _| {
            state.lock().unwrap().enumerated = true;
            Ok(())
        }))?;
    }
    watcher.Start()?;
    Ok(watcher)
}

fn laptop_report() -> windows::core::Result<Option<BatteryDevice>> {
    let report = Battery::AggregateBattery()?.GetReport()?;
    if report.Status()? == BatteryStatus::NotPresent {
        return Ok(None); // a desktop
    }
    let (Ok(remaining), Ok(full)) =
        (report.RemainingCapacityInMilliwattHours(), report.FullChargeCapacityInMilliwattHours())
    else {
        return Ok(None);
    };
    let (remaining, full) = (remaining.Value()?, full.Value()?);
    if full <= 0 {
        return Ok(None);
    }
    let on_ac = PowerManager::PowerSupplyStatus()? != PowerSupplyStatus::NotPresent;
    Ok(Some(BatteryDevice {
        id: "laptop".into(),
        name: "This computer".into(),
        kind: DeviceKind::Laptop,
        percent: ((remaining as f64 / full as f64) * 100.0).round().clamp(0.0, 100.0) as u8,
        charging: on_ac,
        parts: vec![],
    }))
}

fn watch_laptop(sink: Sink) -> windows::core::Result<Battery> {
    let send = {
        let sink = sink.clone();
        move || match laptop_report() {
            Ok(Some(d)) => sink.send(Report::Laptop(d)),
            Ok(None) => {}
            Err(e) => debug_log(&format!("battery report failed: {e}")),
        }
    };
    send();
    let battery = Battery::AggregateBattery()?;
    let on_report = send.clone();
    battery.ReportUpdated(&TypedEventHandler::new(move |_, _| {
        on_report();
        Ok(())
    }))?;
    PowerManager::PowerSupplyStatusChanged(&EventHandler::new(move |_, _| {
        send();
        Ok(())
    }))?;
    Ok(battery)
}

pub fn start(sink: Sink) {
    let _ = std::thread::Builder::new().name("islet-battery".into()).spawn(move || {
        // SAFETY: initialises WinRT for this thread (multithreaded apartment).
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
        // Keep the watcher and battery objects alive for the process lifetime;
        // their callbacks run on the system thread pool.
        let watcher = watch_bluetooth(sink.clone()).map_err(|e| debug_log(&format!("Bluetooth watcher failed: {e}")));
        let battery = watch_laptop(sink).map_err(|e| debug_log(&format!("laptop battery unavailable: {e}")));
        let _keep = (watcher, battery);
        loop {
            std::thread::park();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_kinds() {
        assert_eq!(clean_name("WH-1000XM4 Hands-Free AG"), "WH-1000XM4");
        assert_eq!(kind_from_name("MX Master 3S"), DeviceKind::Mouse);
        assert_eq!(kind_from_name("Galaxy Buds2 Pro"), DeviceKind::Earbuds);
        assert_eq!(kind_from_name("Xbox Wireless Controller"), DeviceKind::Gamepad);
        assert_eq!(kind_from_name("WH-1000XM4"), DeviceKind::Headphones);
    }
}
