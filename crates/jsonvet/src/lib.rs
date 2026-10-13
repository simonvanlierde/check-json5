//! Checks files in the JSON family for syntax errors, duplicate keys and the rules of each
//! dialect. See `docs/design.md` in the repository for the contract.

/// The crate version, which the CLI prints.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
