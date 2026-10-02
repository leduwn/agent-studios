/// Options for configuring Codex Responses bridge runtime flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CodexBridgeOptions {
    /// Whether this provider supports the Responses API WebSocket transport.
    pub supports_websockets: bool,
    /// Whether this provider supports the standalone web-search endpoint.
    pub supports_standalone_web_search: bool,
    /// Runtime-only opt-in for internal metadata.
    pub include_internal_metadata: bool,
}

impl CodexBridgeOptions {
    pub fn new() -> Self {
        Self::default()
    }
}
