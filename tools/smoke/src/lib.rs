//! Issue #15 real smoke run: approved plan, recorded commands, bounded file waits.
pub mod flow;
pub mod prompts;
pub mod run;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
