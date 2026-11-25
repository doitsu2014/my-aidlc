//! Experimental migration code. Project-mutating modules are incomplete and must
//! not be used on real project state until compatibility and safety are verified.

pub mod args;
pub mod artifacts;
pub mod config_read;
pub mod dependencies;
pub mod gate;
pub mod ids;
pub mod knowledge;
pub mod onboarding;
pub mod prerequisites;
pub mod scope;
pub mod state_migration;
pub mod status;
pub mod workflow;
