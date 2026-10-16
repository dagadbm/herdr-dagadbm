//! Helpers shared by the integration tests. Cargo does not build files in a
//! subfolder of `tests/` as their own test, so a test file pulls this in with
//! `mod support;`.

// The unit tests compile this module too and use other parts of it.
#[allow(dead_code)]
pub mod herdr_server;
