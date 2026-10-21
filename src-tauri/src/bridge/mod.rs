//! Browser integration.
//!
//! ```text
//! extension ──native messaging (stdio)──► velora.exe in host mode ──named pipe + token──► running app
//! ```
//!
//! The app never opens a network port. The pipe rejects remote clients, the token lives in the user's
//! profile, and each new extension origin must be approved in the app before it can add downloads.

pub mod host;
pub mod protocol;
#[cfg(windows)]
pub mod server;
