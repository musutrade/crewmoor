//! Real smoke runs (#15 team launch, #6 step 3 Hook forwarding): approved plan,
//! recorded commands, bounded file waits.
pub mod flow;
pub mod forward;
pub mod prompts;
pub mod run;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
