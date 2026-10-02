use agent_studios_provider::{ModelRef, ProviderInstanceId};
use codex_model_provider_info::ModelProviderInfo;

use crate::report::CodexCompatibilityReport;

/// Runtime adapter binding between an Agent Studios ModelRef and Codex Responses provider info.
///
/// NOTE: This type is NOT serialized as persistent configuration.
/// It is an ephemeral runtime adapter product.
#[derive(Clone, Debug, PartialEq)]
pub struct CodexResponsesBinding {
    pub model_ref: ModelRef,
    pub provider_instance_id: ProviderInstanceId,
    pub codex_provider_key: String,
    pub model_id: String,
    pub provider_info: ModelProviderInfo,
    pub compatibility_report: CodexCompatibilityReport,
}
