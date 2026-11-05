//! Wi-Fi through CoreWLAN and Bluetooth through IOBluetooth.
//!
//! Since macOS 14, apps see Wi-Fi network names only with Location
//! permission; Islet asks once, the first time the Wi-Fi list is opened.

use super::connectivity::{Backend, ConnectError};
use islet_core::activity::DeviceKind;
use islet_core::connectivity::{self, BtDevice, BtState, Radio, WifiNetwork, WifiState};
use objc2::rc::Retained;
use objc2_core_wlan::{CWNetwork, CWSecurity, CWWiFiClient};
use objc2_foundation::{NSOperationQueue, NSString};
use objc2_io_bluetooth::{BluetoothHCIPowerState, IOBluetoothDevice, IOBluetoothHostController};
use std::collections::HashSet;
use std::sync::Once;

const LOCATION_NOTE: &str =
    "To see network names, allow Islet in System Settings → Privacy & Security → Location Services";

#[derive(Default)]
pub struct Mac;

/// Ask for Location permission once (it must happen on the main thread,
/// and the manager must stay alive while macOS shows the prompt).
fn request_location() {
    static ASKED: Once = Once::new();
    ASKED.call_once(|| {
        let block = block2::RcBlock::new(|| {
            // SAFETY: on the main queue, as CoreLocation requires.
            unsafe {
                let manager = objc2_core_location::CLLocationManager::new();
                manager.requestWhenInUseAuthorization();
                std::mem::forget(manager);
            }
        });
        // SAFETY: a valid block, run once on the main operation queue.
        unsafe { NSOperationQueue::mainQueue().addOperationWithBlock(&block) };
    });
}

fn networks(set: &objc2_foundation::NSSet<CWNetwork>) -> Vec<Retained<CWNetwork>> {
    set.allObjects().to_vec()
}

impl Backend for Mac {
    fn set_power(&mut self, radio: Radio, on: bool) -> Result<(), String> {
        match radio {
            // SAFETY: CoreWLAN calls with valid receivers.
            Radio::Wifi => unsafe {
                let iface = CWWiFiClient::sharedWiFiClient().interface().ok_or("No Wi-Fi adapter found")?;
                iface.setPower_error(on).map_err(|e| e.localizedDescription().to_string())
            },
            // macOS has no public API for the Bluetooth power switch.
            Radio::Bluetooth => Err("Turn Bluetooth on or off in System Settings".into()),
        }
    }

    fn wifi(&mut self, scan: bool) -> WifiState {
        // SAFETY: CoreWLAN calls with valid receivers; results are retained.
        unsafe {
            let client = CWWiFiClient::sharedWiFiClient();
            let Some(iface) = client.interface() else {
                return WifiState { note: Some("No Wi-Fi adapter found".into()), ..WifiState::default() };
            };
            if !iface.powerOn() {
                return WifiState { available: true, note: Some("Wi-Fi is off".into()), ..WifiState::default() };
            }
            let current = iface.ssid().map(|s| s.to_string());
            let known: HashSet<String> = iface
                .configuration()
                .map(|c| {
                    c.networkProfiles()
                        .array()
                        .to_vec()
                        .iter()
                        .filter_map(|p| p.ssid())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();
            let found = if scan { iface.scanForNetworksWithName_error(None).ok() } else { iface.cachedScanResults() };
            let mut unnamed = 0;
            let mut list = Vec::new();
            for n in found.as_deref().map(networks).unwrap_or_default() {
                let Some(ssid) = n.ssid().map(|s| s.to_string()) else {
                    unnamed += 1;
                    continue;
                };
                list.push(WifiNetwork {
                    strength: connectivity::rssi_to_strength(n.rssiValue() as i32),
                    secure: !n.supportsSecurity(CWSecurity::None),
                    known: known.contains(&ssid),
                    active: current.as_deref() == Some(ssid.as_str()),
                    ssid,
                });
            }
            let note = (list.is_empty() && unnamed > 0).then(|| {
                request_location();
                LOCATION_NOTE.to_string()
            });
            WifiState { available: true, enabled: true, current, networks: connectivity::merge_networks(list), note }
        }
    }

    fn wifi_connect(&mut self, ssid: &str, password: Option<&str>) -> Result<(), ConnectError> {
        // SAFETY: as above; the network object comes from this interface's scan.
        unsafe {
            let iface = CWWiFiClient::sharedWiFiClient()
                .interface()
                .ok_or_else(|| ConnectError::Failed("No Wi-Fi adapter".into()))?;
            let name = NSString::from_str(ssid);
            let set = iface
                .scanForNetworksWithName_error(Some(&name))
                .map_err(|e| ConnectError::Failed(format!("Couldn't scan: {}", e.localizedDescription())))?;
            let net = networks(&set)
                .into_iter()
                .next()
                .ok_or_else(|| ConnectError::Failed(format!("{ssid} is out of range")))?;
            let secure = !net.supportsSecurity(CWSecurity::None);
            let known = iface.configuration().is_some_and(|c| {
                c.networkProfiles().array().to_vec().iter().any(|p| p.ssid().is_some_and(|s| s.to_string() == ssid))
            });
            if secure && !known && password.is_none() {
                return Err(ConnectError::NeedsPassword);
            }
            let pw = password.map(NSString::from_str);
            match iface.associateToNetwork_password_error(&net, pw.as_deref()) {
                Ok(()) => Ok(()),
                Err(_) if secure && password.is_none() => Err(ConnectError::NeedsPassword),
                Err(e) => Err(ConnectError::Failed(format!("Couldn't join {ssid}: {}", e.localizedDescription()))),
            }
        }
    }

    #[allow(deprecated)]
    fn bluetooth(&mut self) -> BtState {
        // SAFETY: IOBluetooth class and instance methods with valid receivers.
        unsafe {
            let Some(controller) = IOBluetoothHostController::defaultController() else {
                return BtState { note: Some("No Bluetooth adapter found".into()), ..BtState::default() };
            };
            let powered = controller.powerState() == BluetoothHCIPowerState::ON;
            let mut devices = Vec::new();
            for obj in IOBluetoothDevice::pairedDevices().map(|a| a.to_vec()).unwrap_or_default() {
                let Ok(d) = obj.downcast::<IOBluetoothDevice>() else { continue };
                let Some(id) = d.addressString().map(|s| s.to_string()) else { continue };
                let name = d.nameOrAddress().map(|s| s.to_string()).unwrap_or_else(|| id.clone());
                let kind = match d.deviceClassMajor() {
                    2 => DeviceKind::Phone,
                    4 => match connectivity::kind_from_hint(&name) {
                        DeviceKind::Other => DeviceKind::Headphones,
                        k => k,
                    },
                    _ => connectivity::kind_from_hint(&name),
                };
                devices.push(BtDevice { id, name, kind, connected: d.isConnected() });
            }
            connectivity::sort_devices(&mut devices);
            BtState {
                available: true,
                powered,
                devices,
                can_connect: true,
                note: (!powered).then(|| "Bluetooth is off".into()),
            }
        }
    }

    fn bt_set(&mut self, id: &str, connect: bool) -> Result<(), String> {
        // SAFETY: the device comes from its address string; calls are synchronous.
        unsafe {
            let d = IOBluetoothDevice::deviceWithAddressString(Some(&NSString::from_str(id)))
                .ok_or_else(|| "That device is no longer paired".to_string())?;
            let r = if connect { d.openConnection() } else { d.closeConnection() };
            if r == 0 {
                Ok(())
            } else {
                Err(format!("Couldn't {} (error {r:#x})", if connect { "connect" } else { "disconnect" }))
            }
        }
    }
}
