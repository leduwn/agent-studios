//! Codex Provider Bridge
//!
//! Compatibility boundary connecting Agent Studios Provider Core to the
//! upstream-derived Codex Responses runtime configuration.

pub mod binding;
pub mod bridge;
pub mod error;
pub mod options;
pub mod report;

pub use binding::CodexResponsesBinding;
pub use bridge::{
    CodexProviderBridge, deterministic_codex_provider_key, resolve_catalog_url,
    validate_header_name,
};
pub use error::CodexBridgeError;
pub use options::CodexBridgeOptions;
pub use report::CodexCompatibilityReport;
