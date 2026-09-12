//! Reproducible comparison benchmarks for the Amber-Store cores.
//!
//! The harness treats every store under test as a black box driven through
//! its own shipped command line, generates one shared deterministic corpus
//! for all of them, and reports what each one cost together with what each
//! one actually guarantees.

pub mod adapters;
pub mod blob;
pub mod config;
pub mod dataset;
pub mod hostinfo;
pub mod metrics;
pub mod publish;
pub mod report;
pub mod run;
pub mod scenarios;
pub mod stats;
pub mod toolbox;
pub mod util;
