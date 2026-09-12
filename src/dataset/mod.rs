//! One shared, deterministic corpus for every backend.
//!
//! The three scenario groups need different shapes of data, but they all draw
//! their bytes from the same generator and they are all described by the same
//! independently computed [`manifest::Manifest`], so "what was stored" and
//! "what came back" are never checked against the generator's own bookkeeping.

pub mod corpus;
pub mod gitgen;
pub mod manifest;
