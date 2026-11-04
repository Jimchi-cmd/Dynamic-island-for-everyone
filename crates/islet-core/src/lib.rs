//! Platform-independent core of Islet. Nothing in here touches an OS API,
//! so everything is unit-testable on any machine.

pub mod clock;
pub mod geometry;
pub mod island;
pub mod spring;
