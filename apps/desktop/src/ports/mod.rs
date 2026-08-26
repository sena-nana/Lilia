//! Host ports whose execution cannot live in a feature crate.
//!
//! GitHub binding drives a device-flow loop and must unbind on cancel.
//! Import runs against a live `DesktopApplicationConfig` and OS file locks.

pub mod github;
pub mod import;
