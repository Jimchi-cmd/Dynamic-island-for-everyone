//! Command-line commands that talk to the running island (or set it up).
//!
//! ```text
//! islet                                   run the island
//! islet nav start <destination> --from <origin> [--label <name>] [--mode drive|two-wheeler|walk|bicycle|transit]
//! islet nav stop
//! islet key routes                        read a Google Routes API key from stdin into the OS keychain
//! ```

use crate::providers::connectivity::{self as conn, ConnectError};
use crate::{autostart, config, control, net, providers, secrets};
use control::Control;
use islet_core::alarm::{self, Alarm};
use islet_core::connectivity::Radio;
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
  islet wifi on|off                        turn Wi-Fi on or off
                                           join a network (the password is read from stdin)
  islet bt list [--json]                   paired Bluetooth devices
  islet bt connect|disconnect <id>         connect or disconnect one (id from bt list)
  islet bt on|off                          turn Bluetooth on or off
  islet weather                            the weather now (if a place is set)
  islet weather set <city>                 show weather for a place (Open-Meteo, no key)
  islet weather search <city> [--json]     find places by name
  islet weather off                        stop fetching weather
  islet calendar [--json]                  your next events (from the calendar link)
  islet calendar set                       read a private iCal (ICS) link from stdin into the
                                           OS keychain and turn calendar on
  islet calendar on|off|clear              show events or not; clear removes the link
  islet activity start|update <id> --title <text> [--subtitle <text>] [--progress 0-100]
                                 [--color #rrggbb]
                                           show your own live activity (a build, a download…)
  islet activity end <id>                  remove it
  islet shelf [--json]                     files on the shelf (drop files on the island to add)
  islet shelf clear                        empty the shelf (the files themselves stay)
  islet spotify login <client-id>          show Spotify playback from your other devices (opens
                                           the browser; register the redirect URI first)
  islet spotify logout|on|off
  islet media hide|show <app>              hide (or show again) one app's Now Playing
  islet set <setting> on|off               volume-hud, brightness-hud, caps-lock-hud,
                                           notifications, calendar, fahrenheit, web-artwork
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
        ["wifi", state @ ("on" | "off")] => set_radio(Radio::Wifi, *state == "on"),
        ["bt", state @ ("on" | "off")] => set_radio(Radio::Bluetooth, *state == "on"),
        ["bt", verb @ ("connect" | "disconnect"), id] => conn::backend()
            .bt_set(id, *verb == "connect")
            .map(|()| format!("{}ed.", if *verb == "connect" { "Connect" } else { "Disconnect" })),
        ["timer", "start", rest @ ..] => parse_timer(rest).and_then(send_timer),
        ["timer", "pause"] => send_timer(TimerCommand::Pause),
        ["timer", "resume"] => send_timer(TimerCommand::Resume),
        ["timer", "stop"] => send_timer(TimerCommand::Stop),
        ["focus", rest @ ..] => parse_focus(rest).and_then(send_timer),
        ["weather"] => weather_now(),
        ["weather", "off"] => set_weather(None).map(|()| "Weather is off.".into()),
        ["weather", "search", rest @ ..] => weather_search(rest),
        ["weather", "set", "--lat", lat, "--lon", lon, "--name", name] => weather_set_exact(lat, lon, name),
        ["weather", "set", rest @ ..] if !rest.is_empty() => weather_set_search(rest),
        ["calendar"] => calendar_list(false),
        ["calendar", "--json"] => calendar_list(true),
        ["calendar", "set"] => calendar_set(),
        ["calendar", "on"] => set_switch("calendar", "on"),
        ["calendar", "off"] => set_switch("calendar", "off"),
        ["calendar", "clear"] => secrets::clear_calendar_url()
            .and_then(|()| set_switch("calendar", "off"))
            .map(|_| "Calendar link removed.".into()),
        ["activity", "start" | "update", id, rest @ ..] => {
            parse_activity(id, rest).and_then(|c| control::send(&Control::Activity(c)).map(|()| String::new()))
        }
        ["activity", "end", id] => control::send(&Control::ActivityEnd(id.to_string())).map(|()| String::new()),
        ["shelf"] => Ok(shelf_list(false)),
        ["shelf", "--json"] => Ok(shelf_list(true)),
        ["shelf", "clear"] => {
            config::save_shelf(&[]);
            let _ = control::send(&Control::ShelfChanged);
            Ok("The shelf is empty.".into())
        }
        ["media", verb @ ("hide" | "show"), app @ ..] if !app.is_empty() => {
            media_exclude(*verb == "hide", &app.join(" "))
        }
        ["spotify", "login", client_id] => {
            providers::spotify::login(client_id).and_then(|m| set_switch("spotify", "on").map(|_| m))
        }
        ["spotify", "logout"] => secrets::clear_spotify()
            .and_then(|()| set_switch("spotify", "off"))
            .map(|_| "Unlinked from Spotify.".into()),
        ["spotify", "on"] => set_switch("spotify", "on"),
        ["spotify", "off"] => set_switch("spotify", "off"),
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

fn set_weather(place: Option<islet_core::prefs::Place>) -> Result<(), String> {
    let mut p = config::load_prefs();
    p.weather = place;
    config::save_prefs(&p)?;
    prefs_changed();
    Ok(())
}

fn weather_set_exact(lat: &str, lon: &str, name: &str) -> Result<String, String> {
    let lat = lat.parse::<f64>().map_err(|_| format!("{lat:?} isn't a latitude"))?;
    let lon = lon.parse::<f64>().map_err(|_| format!("{lon:?} isn't a longitude"))?;
    set_weather(Some(islet_core::prefs::Place { name: name.to_string(), lat, lon }))?;
    Ok(format!("Weather for {name}."))
}

fn weather_set_search(words: &[&str]) -> Result<String, String> {
    let q = words.join(" ");
    let place = providers::weather::search(&q)?.into_iter().next().ok_or_else(|| format!("no place called {q:?}"))?;
    let name = place.name.clone();
    set_weather(Some(place))?;
    Ok(format!("Weather for {name}."))
}

fn weather_now() -> Result<String, String> {
    let p = config::load_prefs();
    let place = p.weather.ok_or("Weather is off. Turn it on with: islet weather set <city>")?;
    let w = providers::weather::fetch(&place)?;
    let t = |c| islet_core::weather::format_temp(c, p.fahrenheit);
    let range = match (w.high_c, w.low_c) {
        (Some(h), Some(l)) => format!(" (high {}, low {})", t(h), t(l)),
        _ => String::new(),
    };
    Ok(format!(
        "{}: {}, {}{}. Weather data by Open-Meteo.com",
        w.place,
        t(w.temp_c),
        islet_core::weather::describe(w.sky),
        range
    ))
}

fn weather_search(args: &[&str]) -> Result<String, String> {
    let json = args.contains(&"--json");
    let q: Vec<&str> = args.iter().copied().filter(|a| *a != "--json").collect();
    if q.is_empty() {
        return Err("search for what? islet weather search Kampala".into());
    }
    let places = providers::weather::search(&q.join(" "))?;
    if json {
        return Ok(serde_json::to_string(&places).unwrap_or_default());
    }
    Ok(if places.is_empty() {
        "No places found.".into()
    } else {
        places.iter().map(|p| format!("{}  ({:.3}, {:.3})", p.name, p.lat, p.lon)).collect::<Vec<_>>().join("\n")
    })
}

fn calendar_set() -> Result<String, String> {
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        eprintln!("Paste your calendar's private iCal (ICS) link and press Enter:");
    }
    let mut link = String::new();
    std::io::stdin().read_line(&mut link).map_err(|e| e.to_string())?;
    let link = link.trim();
    if !(link.starts_with("https://") || link.starts_with("webcal://")) {
        return Err("that doesn't look like a calendar link (it should start with https:// or webcal://)".into());
    }
    secrets::set_calendar_url(link)?;
    set_switch("calendar", "on")?;
    Ok("Saved to the OS keychain; calendar is on.".into())
}

fn calendar_list(json: bool) -> Result<String, String> {
    let events = providers::calendar::fetch()?;
    let off = crate::clock::utc_offset();
    if json {
        let list: Vec<_> = events
            .iter()
            .map(|e| serde_json::json!({ "summary": e.summary, "start": e.start, "end": e.end, "allDay": e.all_day, "location": e.location }))
            .collect();
        return Ok(serde_json::to_string(&list).unwrap_or_default());
    }
    if events.is_empty() {
        return Ok("No events in the next week.".into());
    }
    Ok(events
        .iter()
        .take(15)
        .map(|e| {
            let days = (e.start + off).div_euclid(86_400);
            let (y, m, d) = islet_core::calendar::civil_from_days(days);
            let secs = (e.start + off).rem_euclid(86_400);
            let when =
                if e.all_day { "all day".to_string() } else { format!("{:02}:{:02}", secs / 3600, secs / 60 % 60) };
            format!("{y}-{m:02}-{d:02} {when:>7}  {}", e.summary)
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn parse_activity(id: &str, args: &[&str]) -> Result<islet_core::activity::Custom, String> {
    if id.is_empty() || id.starts_with('-') || id.contains(['\t', '\n']) {
        return Err("give the activity an id: islet activity start build --title Build".into());
    }
    let mut c = islet_core::activity::Custom {
        id: id.into(),
        title: String::new(),
        subtitle: String::new(),
        progress: None,
        color: None,
    };
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        let mut value = |name: &str| it.next().copied().ok_or_else(|| format!("{name} needs a value"));
        match arg {
            "--title" => c.title = value("--title")?.into(),
            "--subtitle" => c.subtitle = value("--subtitle")?.into(),
            "--progress" => {
                let v = value("--progress")?;
                let p: f32 =
                    v.trim_end_matches('%').parse().map_err(|_| format!("{v:?} isn't a number from 0 to 100"))?;
                c.progress = Some((p / 100.0).clamp(0.0, 1.0));
            }
            "--color" => {
                let v = value("--color")?.trim_start_matches('#');
                let n = u32::from_str_radix(v, 16).ok().filter(|_| v.len() == 6).ok_or("--color takes #rrggbb")?;
                c.color = Some([(n >> 16) as u8, (n >> 8) as u8, n as u8]);
            }
            o => return Err(format!("unknown option {o}")),
        }
    }
    if c.title.trim().is_empty() {
        c.title = c.id.clone();
    }
    Ok(c)
}

fn shelf_list(json: bool) -> String {
    let items = config::load_shelf();
    if json {
        return serde_json::to_string(&items).unwrap_or_default();
    }
    if items.is_empty() {
        return "The shelf is empty. Drag files onto the island to keep them handy.".into();
    }
    items.iter().map(|i| i.path.clone()).collect::<Vec<_>>().join("\n")
}

fn media_exclude(hide: bool, app: &str) -> Result<String, String> {
    let mut p = config::load_prefs();
    p.media_exclude.retain(|x| !x.eq_ignore_ascii_case(app));
    if hide {
        p.media_exclude.push(app.to_string());
    }
    config::save_prefs(&p)?;
    prefs_changed();
    Ok(if hide { format!("Now Playing from {app} is hidden.") } else { format!("Now Playing from {app} shows again.") })
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

fn set_radio(radio: Radio, on: bool) -> Result<String, String> {
    conn::backend().set_power(radio, on).map(|()| format!("{} is {}.", radio.name(), if on { "on" } else { "off" }))
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
            "spotifyLinked": providers::spotify::linked(),
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
    fn parses_activity_arguments() {
        let c = parse_activity("build", &["--title", "Build", "--progress", "40%", "--color", "#30d158"]).unwrap();
        assert_eq!((c.title.as_str(), c.progress, c.color), ("Build", Some(0.4), Some([0x30, 0xd1, 0x58])));
        assert_eq!(parse_activity("dl", &[]).unwrap().title, "dl", "the id is the default title");
        assert!(parse_activity("x", &["--color", "red"]).is_err());
        assert!(parse_activity("--title", &[]).is_err());
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
