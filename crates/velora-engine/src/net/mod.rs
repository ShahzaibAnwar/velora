//! HTTP plumbing: client construction, probing, header parsing, retry classification and throttling.

pub mod client;
pub mod content_disposition;
pub mod probe;
pub mod range;
pub mod ratelimit;
pub mod retry;

pub use client::build_client;
pub use probe::probe;
pub use ratelimit::RateLimiter;
pub use retry::Failure;
