//! Building blocks shared by the dataset generators, the adapters and the
//! report writers: a deterministic byte source, a bounded subprocess runner
//! that also reports child CPU time and peak RSS, and filesystem helpers that
//! keep every byte the harness writes inside a directory it created itself.

pub mod fsx;
pub mod proc;
pub mod rng;
