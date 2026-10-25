//! Soldier wall queries and their regression tests.
//!
//! `queries` owns push-out, sweeps, ledges and the per-triangle contact workaround.

mod queries;

pub use queries::{SWEEP_CONTACT_DIST, WALL_ITERATIONS};

#[cfg(test)]
mod tests;
