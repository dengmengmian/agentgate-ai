//! Shared helpers for offline capability fixture tests.
//!
//! Each integration test binary that needs these picks them up via `mod common;`.

#![allow(dead_code)]

pub mod gateway_harness;
pub mod mock_upstream;
