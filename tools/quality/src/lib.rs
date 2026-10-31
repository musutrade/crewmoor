pub mod config;
pub mod coverage;
pub mod driver;
pub mod inventory;
pub mod source;
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
