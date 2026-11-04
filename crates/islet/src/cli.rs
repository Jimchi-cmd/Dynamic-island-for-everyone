//! Command-line commands that talk to the running island (or set it up).
//!
//! ```text
//! islet                                   run the island
//! islet nav start <destination> --from <origin> [--label <name>] [--mode drive|two-wheeler|walk|bicycle|transit]
//! islet nav stop
//! islet key routes                        read a Google Routes API key from stdin into the OS keychain
//! ```

use crate::{autostart, config, control, secrets};
use control::Control;
use islet_core::nav::{TravelMode, Trip};

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
  islet autostart on|off                   start Islet when you log in (or stop doing so)
  islet status [--json]                    is Islet running, and how it is set up
  islet quit                               close the running island
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

fn store_key() -> Result<String, String> {
    eprintln!("Paste your Google Routes API key and press Enter:");
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
    fn island_flags_are_not_commands() {
        assert!(run(&[]).is_none());
        assert!(run(&["--mediaremote-probe".into()]).is_none());
        assert!(run(&["--help".into()]).unwrap().unwrap().contains("nav start"));
        assert!(run(&["bogus".into()]).unwrap().is_err());
        assert!(run(&["--version".into()]).unwrap().unwrap().starts_with("islet "));
    }
}
