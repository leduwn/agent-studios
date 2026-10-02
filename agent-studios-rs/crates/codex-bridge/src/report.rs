/// Runtime capability verification report produced during bridge resolution.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CodexCompatibilityReport {
    /// List of capability names that are in `Unknown` state and thus unverified.
    pub unverified_capabilities: Vec<String>,
}

impl CodexCompatibilityReport {
    pub fn is_fully_verified(&self) -> bool {
        self.unverified_capabilities.is_empty()
    }
}
