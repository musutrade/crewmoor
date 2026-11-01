//! User-level native hooks and explicitly authored business reports.
mod cli;
mod event;
pub mod install;
mod journal;
pub(crate) mod model;
pub use cli::cli;
pub use event::{ingest, report};
pub use journal::show;
pub use model::{Context, Evidence, Registration, register};
pub const INSTANCE_ENV: &str = "CREWMOOR_FEEDBACK_INSTANCE";
