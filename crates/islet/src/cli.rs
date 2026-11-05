//! Command-line commands that talk to the running island (or set it up).
//!
//! ```text
//! islet                                   run the island
//! islet nav start <destination> --from <origin> [--label <name>] [--mode drive|two-wheeler|walk|bicycle|transit]
//! islet nav stop
//! islet key routes                        read a Google Routes API key from stdin into the OS keychain
//! ```

use crate::providers::connectivity::{self as conn, ConnectError};
use crate::{autostart, config, control, net, secrets};
use control::Control;
use islet_core::alarm::{self, Alarm};
use islet_core::nav::{TravelMode, Trip};
use islet_core::timer::{self, TimerCommand};

pub const USAGE: &str = "\
Usage:
  islet                                    run the island
  islet nav start <destination> --from <origin> [--label <name>] [--mode <mode>]
                                           show live ETA to <destination> (modes: drive,
                                           two-wheeler, walk, bicycle, transit)
  islet nav stop                           stop showing the trip
  islet key routes                         store your Google Routes API key in the OS
                                           keychain (reads one line from stdin)
  islet key routes --clear                 remove it
  islet alarm add <time> [--label <name>] [--days <days>]
                                           set an alarm: 7:30, 19:30 or 7:30pm; days:
                                           once (default), daily, weekdays, weekends, mon,wed,…
  islet alarm list [--json]                your alarms
  islet alarm remove|on|off <id>           delete, turn on or turn off an alarm
  islet alarm stop|snooze                  answer a ringing alarm
  islet timer start <duration> [--label <name>]
                                           a countdown on the island: 25m, 1h30m, 90s, 1:30
  islet timer pause|resume|stop
  islet focus [--work 25m] [--break 5m] [--rounds 4]
                                           a focus session: work and breaks, with a chime
  islet wifi list [--scan] [--json]        nearby Wi-Fi networks
  islet wifi connect <name> [--password-stdin]
                                           join a network (the password is read from stdin)
  islet bt list [--json]                   paired Bluetooth devices
  islet bt connect|disconnect <id>         connect or disconnect one (id from bt list)
  islet set <setting> on|off               volume-hud, brightness-hud, caps-lock-hud,
                                           notifications, calendar, fahrenheit
  islet prefs [--json]                     the island's settings
  islet autostart on|off                   start Islet when you log in (or stop doing so)
  islet status [--json]                    is Islet running, and how it is set up
  islet quit                               close the running island
  islet update check [--json]              ask GitHub for the latest release
  islet --version
  islet --help                             this text";

/// Run a command if `args` (without the program name) names one.
/// Returns None when the island itself should run.
pub fn run(args: &[String]) -> Option<Result<String, String>> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    Some(match a.as_slice() {
        [] => return None,
        [flag, ..] if flag.starts_with("--") && !matches!(*flag, "--help" | "-h" | "--version") => return None,
        ["--help" | "-h" | "help", ..] => Ok(USAGE.to_string()),
        ["--version" | "-V"] => Ok(format!("islet {}", env!("CARGO_PKG_VERSION"))),
        ["nav", "start", rest @ ..] => parse_trip(rest).and_then(start_trip),
        ["nav", "stop"] => stop_trip(),
        ["key", "routes"] => store_key(),
        ["key", "routes", "--clear"] => secrets::clear_routes_key().map(|()| "Removed.".into()),
        ["autostart", "on"] => autostart::set(true).map(|()| "Islet will start when you log in.".into()),
        ["autostart", "off"] => autostart::set(false).map(|()| "Islet will no longer start at login.".into()),
        ["status"] => Ok(status(false)),
        ["status", "--json"] => Ok(status(true)),
        ["update", "check"] => update_check(false),
        ["update", "check", "--json"] => update_check(true),
        ["alarm", "add", rest @ ..] => alarm_add(rest),
        ["alarm", "list"] => Ok(alarm_list(false)),
        ["alarm", "list", "--json"] => Ok(alarm_list(true)),
        ["alarm", verb @ ("remove" | "on" | "off"), id] => alarm_edit(verb, id),
        ["alarm", "stop"] => control::send(&Control::AlarmStop).map(|()| "Stopped.".into()),
        ["alarm", "snooze"] => control::send(&Control::AlarmSnooze).map(|()| "Snoozed for 9 minutes.".into()),
        ["wifi", "list", rest @ ..] => Ok(wifi_list(rest.contains(&"--scan"), rest.contains(&"--json"))),
        ["wifi", "connect", ssid] => wifi_connect(ssid, false),
        ["wifi", "connect", ssid, "--password-stdin"] => wifi_connect(ssid, true),
        ["bt", "list", rest @ ..] => Ok(bt_list(rest.contains(&"--json"))),
        ["bt", verb @ ("connect" | "disconnect"), id] => conn::backend()
            .bt_set(id, *verb == "connect")
            .map(|()| format!("{}ed.", if *verb == "connect" { "Connect" } else { "Disconnect" })),
        ["timer", "start", rest @ ..] => parse_timer(rest).and_then(send_timer),
        ["timer", "pause"] => send_timer(TimerCommand::Pause),
        ["timer", "resume"] => send_timer(TimerCommand::Resume),
        ["timer", "stop"] => send_timer(TimerCommand::Stop),
        ["focus", rest @ ..] => parse_focus(rest).and_then(send_timer),
        ["set", key, value] => set_switch(key, value),
        ["prefs"] => Ok(prefs_text(false)),
        ["prefs", "--json"] => Ok(prefs_text(true)),
        ["quit"] => control::send(&Control::Quit).map(|()| "Closed.".into()),
        _ => Err(format!("unknown command\n\n{USAGE}")),
    })
}

pub fn parse_trip(args: &[&str]) -> Result<Trip, String> {
    let mut destination = Vec::new();
    let (mut origin, mut label, mut mode) = (None, String::new(), TravelMode::Drive);
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        let mut value = |name: &str| it.next().copied().ok_or_else(|| format!("{name} needs a value"));
        match arg {
            "--from" => origin = Some(value("--from")?.to_string()),
            "--label" => label = value("--label")?.to_string(),
            "--mode" => {
                let m = value("--mode")?;
                mode = TravelMode::parse(m).ok_or_else(|| format!("unknown travel mode {m:?}"))?;
            }
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            word => destination.push(word),
        }
    }
    let destination = destination.join(" ");
    if destination.trim().is_empty() {
        return Err("say where to: islet nav start <destination> --from <origin>".into());
    }
    let origin = origin
        .filter(|o| !o.trim().is_empty())
        .ok_or("say where from with --from (Islet has no location access; an address or \"lat,lng\" works)")?;
    Ok(Trip { label, destination, origin, mode })
}

fn start_trip(trip: Trip) -> Result<String, String> {
    let title = trip.title().to_string();
    match control::send(&Control::NavStart(trip.clone())) {
        Ok(()) => Ok(format!("Showing the trip to {title} on the island.")),
        Err(_) => {
            // Not running: save it so the island shows it when it starts.
            config::save_trip(Some(&trip));
            Ok(format!("Saved the trip to {title}; it will show when Islet starts."))
        }
    }
}

fn stop_trip() -> Result<String, String> {
    if control::send(&Control::NavStop).is_err() {
        config::save_trip(None);
    }
    Ok("Stopped.".into())
}

pub fn parse_alarm(args: &[&str], id: u32) -> Result<Alarm, String> {
    let mut words = Vec::new();
    let (mut label, mut days) = (String::new(), 0u8);
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        let mut value = |name: &str| it.next().copied().ok_or_else(|| format!("{name} needs a value"));
        match arg {
            "--label" => label = value("--label")?.to_string(),
            "--days" => {
                let d = value("--days")?;
                days =
                    alarm::parse_days(d).ok_or_else(|| format!("unknown days {d:?} (try weekdays or mon,wed,fri)"))?;
            }
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            w => words.push(w),
        }
    }
    let time = words.join(" ");
    let (hour, minute) =
        alarm::parse_time(&time).ok_or_else(|| format!("say when: islet alarm add 7:30 (not {time:?})"))?;
    Ok(Alarm { id, hour, minute, label, days, enabled: true })
}

/// Tell a running island to reload alarms.json (fine if none is running).
fn alarms_changed() {
    let _ = control::send(&Control::AlarmsChanged);
}

fn alarm_add(args: &[&str]) -> Result<String, String> {
    let mut alarms = config::load_alarms();
    let id = alarms.iter().map(|a| a.id).max().unwrap_or(0) + 1;
    let a = parse_alarm(args, id)?;
    let msg = format!(
        "Alarm {id} set for {} ({}){}.",
        alarm::format_hm(a.hour, a.minute, true),
        alarm::describe_days(a.days).to_lowercase(),
        if a.label.is_empty() { String::new() } else { format!(": {}", a.label) }
    );
    alarms.push(a);
    config::save_alarms(&alarms)?;
    alarms_changed();
    Ok(msg)
}

fn alarm_list(json: bool) -> String {
    let alarms = config::load_alarms();
    if json {
        return serde_json::to_string(&alarms).unwrap_or_else(|_| "[]".into());
    }
    if alarms.is_empty() {
        return "No alarms. Add one with: islet alarm add 7:30".into();
    }
    alarms
        .iter()
        .map(|a| {
            format!(
                "{:>3}  {}  {:<14} {}{}",
                a.id,
                alarm::format_hm(a.hour, a.minute, true),
                alarm::describe_days(a.days),
                a.title(),
                if a.enabled { "" } else { "  (off)" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn alarm_edit(verb: &str, id: &str) -> Result<String, String> {
    let id: u32 = id.parse().map_err(|_| format!("{id:?} is not an alarm number (see islet alarm list)"))?;
    let mut alarms = config::load_alarms();
    let pos = alarms.iter().position(|a| a.id == id).ok_or_else(|| format!("no alarm {id}"))?;
    let msg = match verb {
        "remove" => {
            alarms.remove(pos);
            format!("Removed alarm {id}.")
        }
        on => {
            alarms[pos].enabled = on == "on";
            format!("Alarm {id} turned {on}.")
        }
    };
    config::save_alarms(&alarms)?;
    alarms_changed();
    Ok(msg)
}

fn prefs_changed() {
    let _ = control::send(&Control::PrefsChanged);
}

fn set_switch(key: &str, value: &str) -> Result<String, String> {
    let on = islet_core::prefs::parse_on_off(value).ok_or_else(|| format!("say on or off, not {value:?}"))?;
    let mut p = config::load_prefs();
    p.set_switch(key, on)?;
    config::save_prefs(&p)?;
    prefs_changed();
    Ok(format!("{key} is {}.", if on { "on" } else { "off" }))
}

fn prefs_text(json: bool) -> String {
    let p = config::load_prefs();
    if json {
        return serde_json::to_string(&p).unwrap_or_default();
    }
    let yes = |b: bool| if b { "on" } else { "off" };
    format!(
        "volume-hud {}\nbrightness-hud {}\ncaps-lock-hud {}\nnotifications {}\ncalendar {}\nfahrenheit {}\nweather {}",
        yes(p.volume_hud),
        yes(p.brightness_hud),
        yes(p.caps_lock_hud),
        yes(p.notifications),
        yes(p.calendar),
        yes(p.fahrenheit),
        p.weather.as_ref().map_or("off".to_string(), |w| w.name.clone())
    )
}

fn duration(s: &str) -> Result<f64, String> {
    timer::parse_duration(s).ok_or_else(|| format!("{s:?} isn't a duration (try 25m, 1h30m, 90s or 1:30)"))
}

pub fn parse_timer(args: &[&str]) -> Result<TimerCommand, String> {
    let (mut secs, mut label) = (None, String::new());
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        match arg {
            "--label" => label = it.next().ok_or("--label needs a value")?.to_string(),
            o if o.starts_with("--") => return Err(format!("unknown option {o}")),
            d => secs = Some(duration(d)?),
        }
    }
    Ok(TimerCommand::Start { secs: secs.ok_or("how long? islet timer start 25m")?, label })
}

pub fn parse_focus(args: &[&str]) -> Result<TimerCommand, String> {
    let (mut work, mut brk, mut rounds) = (25.0 * 60.0, 5.0 * 60.0, 4u32);
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        let mut value = |name: &str| it.next().copied().ok_or_else(|| format!("{name} needs a value"));
        match arg {
            "--work" => work = duration(value("--work")?)?,
            "--break" => brk = duration(value("--break")?)?,
            "--rounds" => {
                rounds =
                    value("--rounds")?.parse().ok().filter(|r| (1..=12).contains(r)).ok_or("--rounds takes 1 to 12")?
            }
            o => return Err(format!("unknown option {o}")),
        }
    }
    Ok(TimerCommand::Focus { work, brk, rounds })
}

fn send_timer(cmd: TimerCommand) -> Result<String, String> {
    let msg = match &cmd {
        TimerCommand::Start { secs, .. } => format!("Timer set for {}.", timer::format_countdown(*secs)),
        TimerCommand::Focus { work, brk, rounds } => format!(
            "Focus: {} rounds of {} with {} breaks.",
            rounds,
            timer::format_countdown(*work),
            timer::format_countdown(*brk)
        ),
        TimerCommand::Pause => "Paused.".into(),
        TimerCommand::Resume => "Resumed.".into(),
        TimerCommand::Stop => "Stopped.".into(),
    };
    control::send(&Control::Timer(cmd)).map(|()| msg)
}

fn wifi_list(scan: bool, json: bool) -> String {
    let w = conn::backend().wifi(scan);
    if json {
        let nets: Vec<_> = w
            .networks
            .iter()
            .map(|n| serde_json::json!({ "ssid": n.ssid, "strength": n.strength, "secure": n.secure, "known": n.known, "active": n.active }))
            .collect();
        return serde_json::json!({ "available": w.available, "current": w.current, "networks": nets, "note": w.note })
            .to_string();
    }
    let mut lines: Vec<String> = w
        .networks
        .iter()
        .map(|n| {
            format!(
                "{} {:>3}%  {}{}",
                if n.active { "*" } else { " " },
                n.strength,
                n.ssid,
                if n.secure && !n.known { "  (password)" } else { "" }
            )
        })
        .collect();
    if let Some(note) = w.note {
        lines.push(note);
    }
    if lines.is_empty() {
        lines.push("No networks found.".into());
    }
    lines.join("\n")
}

fn wifi_connect(ssid: &str, password_stdin: bool) -> Result<String, String> {
    let password = if password_stdin {
        let mut pw = String::new();
        std::io::stdin().read_line(&mut pw).map_err(|e| e.to_string())?;
        let pw = pw.trim_end_matches(['\r', '\n']).to_string();
        (!pw.is_empty()).then_some(pw)
    } else {
        None
    };
    match conn::backend().wifi_connect(ssid, password.as_deref()) {
        Ok(()) => Ok(format!("Connected to {ssid}.")),
        Err(ConnectError::NeedsPassword) => Err(format!("{ssid} needs a password (use --password-stdin)")),
        Err(ConnectError::Failed(e)) => Err(e),
    }
}

fn bt_list(json: bool) -> String {
    let b = conn::backend().bluetooth();
    if json {
        let devs: Vec<_> = b
            .devices
            .iter()
            .map(|d| serde_json::json!({ "id": d.id, "name": d.name, "connected": d.connected }))
            .collect();
        return serde_json::json!({ "available": b.available, "powered": b.powered, "canConnect": b.can_connect, "devices": devs, "note": b.note })
            .to_string();
    }
    let mut lines: Vec<String> =
        b.devices.iter().map(|d| format!("{} {}  ({})", if d.connected { "*" } else { " " }, d.name, d.id)).collect();
    if let Some(note) = b.note {
        lines.push(note);
    }
    if lines.is_empty() {
        lines.push("No paired devices.".into());
    }
    lines.join("\n")
}

fn status(json: bool) -> String {
    let running = control::send(&Control::Ping).is_ok();
    let autostart = autostart::is_on();
    let key = secrets::routes_key().is_ok();
    let trip = config::load_trip();
    if json {
        return serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "running": running,
            "autostart": autostart,
            "routesKey": key,
            "trip": trip,
            "alarms": config::load_alarms(),
        })
        .to_string();
    }
    let yes = |b: bool| if b { "yes" } else { "no" };
    format!(
        "Running: {}\nStarts at login: {}\nGoogle Routes key: {}\nTrip: {}",
        yes(running),
        yes(autostart),
        if key { "saved" } else { "not set" },
        trip.as_ref().map_or("none".to_string(), |t| format!("{} (from {})", t.title(), t.origin))
    )
}

fn update_check(json: bool) -> Result<String, String> {
    let current = env!("CARGO_PKG_VERSION");
    let (latest, url) = net::latest_release()?;
    let newer = islet_core::version::is_newer(&latest, current);
    Ok(if json {
        serde_json::json!({ "current": current, "latest": latest, "newer": newer, "url": url }).to_string()
    } else if newer {
        format!("Islet {latest} is available (you have {current}): {url}")
    } else {
        format!("Islet {current} is up to date.")
    })
}

fn store_key() -> Result<String, String> {
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        eprintln!("Paste your Google Routes API key and press Enter:");
    }
    let mut key = String::new();
    std::io::stdin().read_line(&mut key).map_err(|e| e.to_string())?;
    let key = key.trim();
    if key.is_empty() {
        return Err("no key given".into());
    }
    secrets::set_routes_key(key)?;
    Ok("Saved to the OS keychain.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_trip_arguments() {
        let t = parse_trip(&["Plot", "5,", "Kampala", "Road", "--from", "Ntinda", "--label", "Work", "--mode", "boda"])
            .unwrap();
        assert_eq!(t.destination, "Plot 5, Kampala Road");
        assert_eq!((t.origin.as_str(), t.label.as_str(), t.mode), ("Ntinda", "Work", TravelMode::TwoWheeler));
        assert!(parse_trip(&["Work"]).unwrap_err().contains("--from"));
        assert!(parse_trip(&["--from", "Ntinda"]).unwrap_err().contains("where to"));
        assert!(parse_trip(&["X", "--mode", "rocket", "--from", "Y"]).is_err());
    }

    #[test]
    fn parses_alarm_arguments() {
        let a = parse_alarm(&["6:45", "am", "--label", "Gym", "--days", "weekdays"], 3).unwrap();
        assert_eq!((a.id, a.hour, a.minute, a.label.as_str(), a.days), (3, 6, 45, "Gym", alarm::WEEKDAYS));
        assert_eq!(parse_alarm(&["21:00"], 1).unwrap().days, 0, "once by default");
        assert!(parse_alarm(&["soon"], 1).is_err());
        assert!(parse_alarm(&["7:00", "--days", "funday"], 1).is_err());
    }

    #[test]
    fn parses_timer_and_focus_arguments() {
        assert_eq!(
            parse_timer(&["10m", "--label", "Tea"]).unwrap(),
            TimerCommand::Start { secs: 600.0, label: "Tea".into() }
        );
        assert!(parse_timer(&["--label", "x"]).is_err());
        assert_eq!(
            parse_focus(&["--work", "50m", "--rounds", "2"]).unwrap(),
            TimerCommand::Focus { work: 3000.0, brk: 300.0, rounds: 2 }
        );
        assert!(parse_focus(&["--rounds", "99"]).is_err());
    }

    #[test]
    fn island_flags_are_not_commands() {
        assert!(run(&[]).is_none());
        assert!(run(&["--mediaremote-probe".into()]).is_none());
        assert!(run(&["--help".into()]).unwrap().unwrap().contains("nav start"));
        assert!(run(&["bogus".into()]).unwrap().is_err());
        assert!(run(&["--version".into()]).unwrap().unwrap().starts_with("islet "));
    }
}
