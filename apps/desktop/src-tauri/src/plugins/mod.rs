//! Lilia plugin / skill surface.
//!
//! Official Claude Code / Codex config management was removed. Agent extensions
//! now go through LiliaCore / AgentKit native registries when available.
//! This module keeps Tauri command names stable with empty overviews so the UI
//! can show an honest empty state.

mod commands;
mod hooks;
mod runtime;
mod types;

pub use commands::*;
