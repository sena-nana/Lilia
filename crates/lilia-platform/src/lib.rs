//! OS ports for LiliaCode.
//!
//! Everything here talks to the operating system and nothing here knows what a
//! project, task or agent is. Product policy — which credential keys may be
//! imported, what a dialog is used for, which update channel exists — stays
//! with the caller.

pub mod clipboard;
pub mod credential;
pub mod dialog;
pub mod launcher;
pub mod power;

mod error;

pub use clipboard::ClipboardImage;
pub use credential::CredentialEntry;
pub use dialog::{FileDialogRequest, FileFilter};
pub use error::{PlatformError, PlatformResult};
