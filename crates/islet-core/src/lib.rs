//! Platform-independent core of Islet. Nothing in here touches an OS API,
//! so everything is unit-testable on any machine.

pub mod activity;
pub mod alarm;
pub mod calendar;
pub mod clipboard;
pub mod clock;
pub mod connectivity;
pub mod downloads;
pub mod geometry;
pub mod hotkey;
pub mod island;
pub mod lyrics;
pub mod nav;
pub mod paths;
pub mod prefs;
pub mod pro;
pub mod shelf;
pub mod spring;
pub mod stopwatch;
pub mod sysmon;
pub mod timer;
pub mod todo;
pub mod version;
pub mod weather;
pub mod worldclock;
