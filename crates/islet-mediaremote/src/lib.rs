//! Islet's macOS Now Playing helper.
//!
//! Since macOS 15.4, `mediaremoted` only answers clients with the right
//! entitlements, and ordinary apps are refused. The system perl
//! (`/usr/bin/perl`) is still allowed, so Islet runs
//!
//! ```text
//! /usr/bin/perl -e '<load this library with DynaLoader and call islet_mediaremote_main>'
//! ```
//!
//! and this library, running inside perl, reads the Now Playing state of
//! whatever app is playing (browsers included, with artwork) and streams
//! it to Islet over stdout. Islet writes commands on stdin; EOF on stdin
//! (Islet quit) ends the helper.
//!
//! Wire format (one line per message, fields separated by TAB):
//! - `N` — nothing is playing
//! - `R` followed by `key=value` fields — title, artist, album, bundle,
//!   playing (0/1), elapsed, duration, rate (seconds), art (base64), art_mime
//! - `E` followed by a message — the helper could not start
//!
//! Values escape `\` as `\\`, TAB as `\t` and newline as `\n`.
//! Commands on stdin: `toggle`, `play`, `pause`, `next`, `previous`,
//! `seek <seconds>`.
//!
//! The MediaRemote API used here is the private framework's C interface;
//! symbols are resolved at runtime so a missing one degrades instead of
//! failing to load.

pub mod wire;

#[cfg(target_os = "macos")]
mod mac;

/// Entry point called from perl (as an XSUB, so the arguments perl passes
/// are ignored). Never returns: the helper exits when Islet goes away.
#[no_mangle]
pub extern "C" fn islet_mediaremote_main() {
    #[cfg(target_os = "macos")]
    mac::run();
    #[cfg(not(target_os = "macos"))]
    {
        println!("E\tnot macOS");
        std::process::exit(2);
    }
}
