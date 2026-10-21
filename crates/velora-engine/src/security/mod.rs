//! Input validation and privacy helpers. Everything that turns untrusted input (URLs, server-supplied
//! file names, user-typed paths) into something the engine acts on goes through this module.

pub mod filename;
pub mod path;
pub mod redact;
pub mod url;

pub use filename::{sanitize_file_name, unique_file_name};
pub use path::{confine, validate_dest_dir};
pub use redact::redact_url;
pub use url::parse_download_url;
