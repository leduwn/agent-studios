use std::fmt;
use std::sync::Mutex;

/// Severity levels for runtime transport diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

/// A structured diagnostic entry emitted during routing or transport execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeDiagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    pub model: Option<String>,
    pub provider_instance_id: Option<String>,
}

/// Interface for recording or observing transport diagnostics without exposing sensitive data.
pub trait RuntimeDiagnosticSink: Send + Sync + fmt::Debug {
    fn emit(&self, diagnostic: RuntimeDiagnostic);
}

/// Diagnostic sink that discards all events.
#[derive(Debug, Default)]
pub struct NoopRuntimeDiagnosticSink;

impl RuntimeDiagnosticSink for NoopRuntimeDiagnosticSink {
    fn emit(&self, _diagnostic: RuntimeDiagnostic) {}
}

/// Diagnostic sink that records emitted events in memory for inspection.
#[derive(Debug, Default)]
pub struct RecordingRuntimeDiagnosticSink {
    diagnostics: Mutex<Vec<RuntimeDiagnostic>>,
}

impl RecordingRuntimeDiagnosticSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> Vec<RuntimeDiagnostic> {
        let guard = self.diagnostics.lock().unwrap();
        guard.clone()
    }

    pub fn clear(&self) {
        let mut guard = self.diagnostics.lock().unwrap();
        guard.clear();
    }
}

impl RuntimeDiagnosticSink for RecordingRuntimeDiagnosticSink {
    fn emit(&self, diagnostic: RuntimeDiagnostic) {
        let mut guard = self.diagnostics.lock().unwrap();
        guard.push(diagnostic);
    }
}
