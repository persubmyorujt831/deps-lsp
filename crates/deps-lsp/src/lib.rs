pub mod cache;
pub mod cargo;
pub mod config;
pub mod document;
pub mod error;
pub mod handlers;
pub mod server;

// Re-export commonly used types
pub use error::{DepsError, Result};
pub use server::Backend;
