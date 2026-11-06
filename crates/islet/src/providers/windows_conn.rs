//! Wi-Fi through the WinRT WiFiAdapter API; Bluetooth devices listed through
//! device enumeration. Windows offers apps no public way to connect a paired
//! Bluetooth device, so the island opens Bluetooth settings for that.

use super::connectivity::{Backend, ConnectError};
use islet_core::connectivity::{self, BtDevice, BtState, Radio, WifiNetwork, WifiState};
use std::collections::HashSet;
use windows::core::HSTRING;
use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice, BluetoothLEDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Devices::WiFi::{
    WiFiAccessStatus, WiFiAdapter, WiFiAvailableNetwork, WiFiConnectionStatus, WiFiReconnectionKind,
};
use windows::Networking::Connectivity::{NetworkAuthenticationType, NetworkInformation};
use windows::Security::Credentials::PasswordCredential;

const ACCESS_NOTE: &str = "Allow Islet to see Wi-Fi networks: Settings → Privacy & security → Location";

#[derive(Default)]
pub struct Windows;

fn adapter() -> Result<WiFiAdapter, String> {
    let access = WiFiAdapter::RequestAccessAsync().and_then(|a| a.join()).map_err(|e| e.message())?;
    if access != WiFiAccessStatus::Allowed {
        return Err(ACCESS_NOTE.into());
    }
    let list = WiFiAdapter::FindAllAdaptersAsync().and_then(|a| a.join()).map_err(|e| e.message())?;
    list.GetAt(0).map_err(|_| "No Wi-Fi adapter found".to_string())
}

/// Wi-Fi profiles saved on this PC (a profile's name is its network name).
fn saved() -> HashSet<String> {
    let Ok(profiles) = NetworkInformation::GetConnectionProfiles() else { return HashSet::new() };
    profiles
        .into_iter()
        .filter(|p| p.IsWlanConnectionProfile().unwrap_or(false))
        .filter_map(|p| p.ProfileName().ok().map(|n| n.to_string()))
        .collect()
}

fn current(a: &WiFiAdapter) -> Option<String> {
    let profile = a.NetworkAdapter().ok()?.GetConnectedProfileAsync().ok()?.join().ok()?;
    let ssid = profile.WlanConnectionProfileDetails().ok()?.GetConnectedSsid().ok()?.to_string();
    (!ssid.is_empty()).then_some(ssid)
}

fn secure(n: &WiFiAvailableNetwork) -> bool {
    n.SecuritySettings()
        .and_then(|s| s.NetworkAuthenticationType())
        .is_ok_and(|t| t != NetworkAuthenticationType::Open80211 && t != NetworkAuthenticationType::None)
}

fn available(a: &WiFiAdapter) -> Vec<WiFiAvailableNetwork> {
    a.NetworkReport().and_then(|r| r.AvailableNetworks()).map(|v| v.into_iter().collect()).unwrap_or_default()
}

/// Whether the computer's radio of this kind is on (None: there is none,
/// or Windows won't say).
fn radio_on(radio: Radio) -> Option<bool> {
    use windows::Devices::Radios::{Radio as WinRadio, RadioKind, RadioState};
    let kind = match radio {
        Radio::Wifi => RadioKind::WiFi,
        Radio::Bluetooth => RadioKind::Bluetooth,
    };
    let radios = WinRadio::GetRadiosAsync().and_then(|a| a.join()).ok()?;
    let states: Vec<RadioState> =
        radios.into_iter().filter(|r| r.Kind().is_ok_and(|k| k == kind)).filter_map(|r| r.State().ok()).collect();
    (!states.is_empty()).then(|| states.contains(&RadioState::On))
}

/// Turn a radio on or off through Windows.Devices.Radios (the same switch
/// as the Quick Settings tiles).
fn set_radio(radio: Radio, on: bool) -> Result<(), String> {
    use windows::Devices::Radios::{Radio as WinRadio, RadioAccessStatus, RadioKind, RadioState};
    let access = WinRadio::RequestAccessAsync().and_then(|a| a.join()).map_err(|e| e.message())?;
    if access != RadioAccessStatus::Allowed {
        return Err(format!("Windows didn't let Islet switch {}", radio.name()));
    }
    let kind = match radio {
        Radio::Wifi => RadioKind::WiFi,
        Radio::Bluetooth => RadioKind::Bluetooth,
    };
    let radios = WinRadio::GetRadiosAsync().and_then(|a| a.join()).map_err(|e| e.message())?;
    let mut found = false;
    for r in radios.into_iter().filter(|r| r.Kind().is_ok_and(|k| k == kind)) {
        found = true;
        let state = if on { RadioState::On } else { RadioState::Off };
        let status = r.SetStateAsync(state).and_then(|a| a.join()).map_err(|e| e.message())?;
        if status != RadioAccessStatus::Allowed {
            return Err(format!("Windows didn't let Islet switch {}", radio.name()));
        }
    }
    if found {
        Ok(())
    } else {
        Err(format!("No {} radio found", radio.name()))
    }
}

impl Backend for Windows {
    fn set_power(&mut self, radio: Radio, on: bool) -> Result<(), String> {
        set_radio(radio, on)
    }

    fn wifi(&mut self, scan: bool) -> WifiState {
        // Wi-Fi switched off: say so (the switch shows off) rather than scan.
        if radio_on(Radio::Wifi) == Some(false) {
            return WifiState { available: true, enabled: false, ..WifiState::default() };
        }
        let a = match adapter() {
            Ok(a) => a,
            Err(note) => return WifiState { available: note == ACCESS_NOTE, note: Some(note), ..WifiState::default() },
        };
        if scan {
            let _ = a.ScanAsync().and_then(|s| s.join());
        }
        let (now, known) = (current(&a), saved());
        let networks = available(&a)
            .into_iter()
            .filter_map(|n| {
                let ssid = n.Ssid().ok()?.to_string();
                Some(WifiNetwork {
                    strength: n.SignalBars().unwrap_or(0).min(4) * 25,
                    secure: secure(&n),
                    known: known.contains(&ssid),
                    active: now.as_deref() == Some(ssid.as_str()),
                    ssid,
                })
            })
            .collect();
        WifiState {
            available: true,
            enabled: true,
            current: now,
            networks: connectivity::merge_networks(networks),
            note: None,
        }
    }

    fn wifi_connect(&mut self, ssid: &str, password: Option<&str>) -> Result<(), ConnectError> {
        let a = adapter().map_err(ConnectError::Failed)?;
        let find = |a: &WiFiAdapter| available(a).into_iter().find(|n| n.Ssid().is_ok_and(|s| s == ssid));
        let n = match find(&a) {
            Some(n) => n,
            None => {
                let _ = a.ScanAsync().and_then(|s| s.join());
                find(&a).ok_or_else(|| ConnectError::Failed(format!("{ssid} is out of range")))?
            }
        };
        let is_secure = secure(&n);
        if is_secure && password.is_none() && !saved().contains(ssid) {
            return Err(ConnectError::NeedsPassword);
        }
        let fail = |e: windows::core::Error| ConnectError::Failed(e.message());
        let result = match password {
            Some(p) => {
                let cred = PasswordCredential::new().map_err(fail)?;
                cred.SetPassword(&HSTRING::from(p)).map_err(fail)?;
                a.ConnectWithPasswordCredentialAsync(&n, WiFiReconnectionKind::Automatic, &cred)
            }
            None => a.ConnectAsync(&n, WiFiReconnectionKind::Automatic),
        }
        .and_then(|op| op.join())
        .map_err(fail)?;
        match result.ConnectionStatus().map_err(fail)? {
            WiFiConnectionStatus::Success => Ok(()),
            WiFiConnectionStatus::InvalidCredential if password.is_none() => Err(ConnectError::NeedsPassword),
            WiFiConnectionStatus::InvalidCredential => {
                Err(ConnectError::Failed(format!("Couldn't join {ssid}. Check the password.")))
            }
            _ => Err(ConnectError::Failed(format!("Couldn't join {ssid}"))),
        }
    }

    fn bluetooth(&mut self) -> BtState {
        let mut devices = Vec::new();
        let mut seen = HashSet::new();
        let selectors = [
            BluetoothDevice::GetDeviceSelectorFromPairingState(true),
            BluetoothLEDevice::GetDeviceSelectorFromPairingState(true),
        ];
        for (le, selector) in selectors.into_iter().enumerate() {
            let Ok(selector) = selector else { continue };
            let Ok(infos) = DeviceInformation::FindAllAsyncAqsFilter(&selector).and_then(|op| op.join()) else {
                continue;
            };
            for info in infos {
                let (Ok(id), Ok(name)) = (info.Id(), info.Name()) else { continue };
                let name = name.to_string();
                if name.is_empty() || !seen.insert(name.clone()) {
                    continue;
                }
                let connected = if le == 1 {
                    BluetoothLEDevice::FromIdAsync(&id)
                        .and_then(|op| op.join())
                        .and_then(|d| d.ConnectionStatus())
                        .is_ok_and(|s| s == BluetoothConnectionStatus::Connected)
                } else {
                    BluetoothDevice::FromIdAsync(&id)
                        .and_then(|op| op.join())
                        .and_then(|d| d.ConnectionStatus())
                        .is_ok_and(|s| s == BluetoothConnectionStatus::Connected)
                };
                devices.push(BtDevice {
                    id: id.to_string(),
                    kind: connectivity::kind_from_hint(&name),
                    name,
                    connected,
                });
            }
        }
        connectivity::sort_devices(&mut devices);
        let radio = radio_on(Radio::Bluetooth);
        let powered = radio.unwrap_or(true);
        if !powered {
            for d in &mut devices {
                d.connected = false;
            }
        }
        BtState {
            available: radio.is_some() || !devices.is_empty(),
            powered,
            devices,
            can_connect: false,
            note: Some("Windows connects Bluetooth devices in its own settings".into()),
        }
    }

    fn bt_set(&mut self, _id: &str, _connect: bool) -> Result<(), String> {
        Err("Connect it in Bluetooth settings".into())
    }
}
