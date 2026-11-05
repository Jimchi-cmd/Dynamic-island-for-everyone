//! Platform-independent core of Islet. Nothing in here touches an OS API,
//! so everything is unit-testable on any machine.

pub mod activity;
pub mod alarm;
pub mod calendar;
pub mod clipboard;
pub mod clock;
pub mod connectivity;
pub mod geometry;
pub mod island;
pub mod nav;
pub mod paths;
pub mod prefs;
pub mod shelf;
pub mod spring;
pub mod sysmon;
pub mod timer;
pub mod version;
pub mod weather;
