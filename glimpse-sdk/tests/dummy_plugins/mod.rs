//! Comprehensive dummy plugin implementations for testing all scenarios
//!
//! This module provides various plugin implementations that can simulate
//! different behaviors needed for thorough testing of the run_plugin function.

pub mod basic_plugin;
pub mod configurable_plugin;
pub mod error_plugin;
pub mod flaky_plugin;
pub mod panic_plugin;
pub mod slow_plugin;

pub use basic_plugin::*;
pub use configurable_plugin::*;
pub use error_plugin::*;
pub use flaky_plugin::*;
pub use panic_plugin::*;
pub use slow_plugin::*;