//! Providers: background readers of OS state (media sessions, battery
//! levels). Each runs on its own thread, blocks on OS events (never polls),
//! and hands reports to the island through a channel plus the backend's
//! waker. Commands for the media session travel the other way.

pub mod art;
mod demo;

use crate::platform::Waker;
use islet_core::activity::{MediaCommand, Report};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Instant;

/// Where providers send reports. Cloneable across threads.
#[derive(Clone)]
pub struct Sink {
    tx: Sender<(Report, Instant)>,
    waker: Waker,
}

impl Sink {
    /// Send a report, stamped now (media positions are relative to this).
    pub fn send(&self, report: Report) {
        if self.tx.send((report, Instant::now())).is_ok() {
            self.waker.wake();
        }
    }
}

pub struct Providers {
    pub reports: Receiver<(Report, Instant)>,
    media: Option<Sender<MediaCommand>>,
}

impl Providers {
    /// Start the providers for this OS (or the scripted demo when
    /// `ISLET_DEMO=media`).
    pub fn start(waker: Waker) -> Self {
        let (tx, reports) = channel();
        let sink = Sink { tx, waker };
        if std::env::var("ISLET_DEMO").as_deref() == Ok("media") {
            return Self { reports, media: Some(demo::start(sink)) };
        }
        let media = platform_start(sink);
        Self { reports, media }
    }

    pub fn command(&self, cmd: MediaCommand) {
        if let Some(tx) = &self.media {
            let _ = tx.send(cmd);
        }
    }
}

#[cfg(target_os = "linux")]
fn platform_start(_sink: Sink) -> Option<Sender<MediaCommand>> {
    None
}

#[cfg(target_os = "windows")]
fn platform_start(_sink: Sink) -> Option<Sender<MediaCommand>> {
    None
}

#[cfg(target_os = "macos")]
fn platform_start(_sink: Sink) -> Option<Sender<MediaCommand>> {
    None
}
