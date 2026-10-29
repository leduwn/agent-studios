use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::AuthenticationScheme;
use crate::catalog::ProviderCatalog;
use crate::definition::ProviderDefinition;
use crate::endpoint::EndpointProfile;
use crate::error::ProviderError;
use crate::id::{ProviderId, ProviderInstanceId};
use crate::instance::ProviderInstance;
use crate::model::ModelDescriptor;
use crate::protocol::ProtocolFamily;
use crate::secret::SecretReference;
use crate::snapshot::{PROVIDER_CATALOG_SCHEMA_VERSION, ProviderCatalogSnapshot};

/// Supported external and native configuration source formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceFormat {
    /// Native Agent Studios snapshot JSON.
    AgentStudios,
    /// Codex CLI config.toml.
    Codex,
    /// OpenCode config JSON.
    OpenCode,
}

impl fmt::Display for SourceFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AgentStudios => write!(f, "agent-studios"),
            Self::Codex => write!(f, "codex"),
            Self::OpenCode => write!(f, "opencode"),
        }
    }
}

impl FromStr for SourceFormat {
    type Err = ProviderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "agent-studios" | "agentstudios" | "snapshot" => Ok(Self::AgentStudios),
            "codex" | "codex-toml" | "toml" => Ok(Self::Codex),
            "opencode" | "open-code" | "json" => Ok(Self::OpenCode),
            other => Err(ProviderError::InvalidConfiguration(format!(
                "Unknown source format: '{other}' (expected 'agent-studios', 'codex', or 'opencode')"
            ))),
        }
    }
}

/// Strategy for resolving conflicts when an imported provider instance matches an existing one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictResolutionStrategy {
    /// Replace existing instance with imported configuration.
    #[default]
    Overwrite,
    /// Retain existing instance, skip imported one.
    Skip,
    /// Fail the entire import if any conflict exists.
    Error,
}

impl fmt::Display for ConflictResolutionStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Overwrite => write!(f, "overwrite"),
            Self::Skip => write!(f, "skip"),
            Self::Error => write!(f, "error"),
        }
    }
}

impl FromStr for ConflictResolutionStrategy {
    type Err = ProviderError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "overwrite" => Ok(Self::Overwrite),
            "skip" => Ok(Self::Skip),
            "error" => Ok(Self::Error),
            other => Err(ProviderError::InvalidConfiguration(format!(
                "Unknown conflict resolution strategy: '{other}' (expected 'overwrite', 'skip', or 'error')"
            ))),
        }
    }
}

/// Details of a conflict between an existing provider instance and an incoming imported instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportConflict {
    pub instance_id: ProviderInstanceId,
    pub existing_name: String,
    pub incoming_name: String,
    pub reason: String,
}

/// Dry-run preview of planned changes before applying an import to the catalog.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportPreview {
    pub added: Vec<ProviderInstance>,
    pub updated: Vec<ProviderInstance>,
    pub skipped: Vec<ProviderInstance>,
    pub conflicts: Vec<ImportConflict>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub definitions: Vec<ProviderDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<ModelDescriptor>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub default_instances: BTreeMap<ProviderId, ProviderInstanceId>,
}

/// Summary of an import operation applied to the catalog.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportResult {
    pub added_count: usize,
    pub updated_count: usize,
    pub skipped_count: usize,
    pub added: Vec<ProviderInstanceId>,
    pub updated: Vec<ProviderInstanceId>,
    pub skipped: Vec<ProviderInstanceId>,
}

/// Trait implemented by configuration format importers.
pub trait ProviderConfigImporter: Send + Sync {
    /// Returns the source format handled by this importer.
    fn format(&self) -> SourceFormat;

    /// Generates an import preview against the specified catalog using a conflict resolution strategy.
    fn preview(
        &self,
        catalog: &ProviderCatalog,
        content: &str,
        strategy: ConflictResolutionStrategy,
    ) -> Result<ImportPreview, ProviderError>;
}

/// Importer for native Agent Studios catalog snapshots (JSON).
#[derive(Clone, Debug, Default)]
pub struct AgentStudiosSnapshotImporter;

impl ProviderConfigImporter for AgentStudiosSnapshotImporter {
    fn format(&self) -> SourceFormat {
        SourceFormat::AgentStudios
    }

    fn preview(
        &self,
        catalog: &ProviderCatalog,
        content: &str,
        strategy: ConflictResolutionStrategy,
    ) -> Result<ImportPreview, ProviderError> {
        let snapshot: ProviderCatalogSnapshot = serde_json::from_str(content)
            .map_err(|e| ProviderError::ImportParseError(e.to_string()))?;

        if snapshot.schema_version != PROVIDER_CATALOG_SCHEMA_VERSION {
            return Err(ProviderError::UnsupportedSchemaVersion {
                version: snapshot.schema_version,
                supported: PROVIDER_CATALOG_SCHEMA_VERSION,
            });
        }

        // Validate definitions
        for def in &snapshot.definitions {
            def.validate()?;
        }

        let mut preview = ImportPreview {
            definitions: snapshot.definitions,
            models: snapshot.models,
            default_instances: snapshot.default_instances,
            ..Default::default()
        };

        categorize_instances(catalog, snapshot.instances, strategy, &mut preview)?;
        Ok(preview)
    }
}

/// Importer for OpenAI Codex CLI configuration files (`config.toml`).
#[derive(Clone, Debug, Default)]
pub struct CodexConfigImporter;

impl ProviderConfigImporter for CodexConfigImporter {
    fn format(&self) -> SourceFormat {
        SourceFormat::Codex
    }

    fn preview(
        &self,
        catalog: &ProviderCatalog,
        content: &str,
        strategy: ConflictResolutionStrategy,
    ) -> Result<ImportPreview, ProviderError> {
        let instances = parse_codex_toml(catalog, content)?;
        let mut preview = ImportPreview::default();
        categorize_instances(catalog, instances, strategy, &mut preview)?;
        Ok(preview)
    }
}

/// Importer for OpenCode configuration files (JSON).
#[derive(Clone, Debug, Default)]
pub struct OpenCodeConfigImporter;

impl ProviderConfigImporter for OpenCodeConfigImporter {
    fn format(&self) -> SourceFormat {
        SourceFormat::OpenCode
    }

    fn preview(
        &self,
        catalog: &ProviderCatalog,
        content: &str,
        strategy: ConflictResolutionStrategy,
    ) -> Result<ImportPreview, ProviderError> {
        let instances = parse_opencode_json(catalog, content)?;
        let mut preview = ImportPreview::default();
        categorize_instances(catalog, instances, strategy, &mut preview)?;
        Ok(preview)
    }
}

/// Generates a dry-run preview of an import operation from raw configuration content.
pub fn preview_import(
    catalog: &ProviderCatalog,
    source_format: SourceFormat,
    content: &str,
    strategy: ConflictResolutionStrategy,
) -> Result<ImportPreview, ProviderError> {
    match source_format {
        SourceFormat::AgentStudios => {
            AgentStudiosSnapshotImporter.preview(catalog, content, strategy)
        }
        SourceFormat::Codex => CodexConfigImporter.preview(catalog, content, strategy),
        SourceFormat::OpenCode => OpenCodeConfigImporter.preview(catalog, content, strategy),
    }
}

/// Performs a staged atomic commit of an import preview into the catalog.
///
/// Invariant: Fails closed with rollback on any invariant violation.
/// If `strategy` is `Error` and any conflicts exist in `preview`, returns `ImportConflict`.
pub fn apply_import(
    catalog: &mut ProviderCatalog,
    preview: ImportPreview,
    strategy: ConflictResolutionStrategy,
) -> Result<ImportResult, ProviderError> {
    if strategy == ConflictResolutionStrategy::Error && !preview.conflicts.is_empty() {
        return Err(ProviderError::ImportConflict(format!(
            "Import aborted: {} conflict(s) encountered under Error strategy",
            preview.conflicts.len()
        )));
    }

    // Atomic execution via temporary staged clone.
    // If any operation fails, staged is dropped and catalog remains untouched.
    let mut staged = catalog.clone();
    let mut result = ImportResult::default();

    // 1. Register any missing definitions required by preview
    for def in preview.definitions {
        if !staged.provider_registry().contains_definition(&def.id) {
            staged.register_provider_definition(def)?;
        }
    }

    // 2. Apply updated instances
    for inst in preview.updated {
        if staged.get_provider_instance(&inst.id).is_some() {
            staged.update_provider_instance(inst.clone())?;
        } else {
            staged.register_provider_instance(inst.clone())?;
        }
        result.updated.push(inst.id);
    }

    // 3. Apply newly added instances
    for inst in preview.added {
        staged.register_provider_instance(inst.clone())?;
        result.added.push(inst.id);
    }

    // 4. Record skipped instances
    for inst in preview.skipped {
        result.skipped.push(inst.id);
    }

    // 5. Register associated models if provided
    for model in preview.models {
        if !staged
            .model_registry()
            .contains(&model.provider_instance_id, &model.id)
            && staged
                .provider_registry()
                .contains_instance(&model.provider_instance_id)
        {
            staged.register_model(model)?;
        }
    }

    // 6. Apply default instances if provided
    for (prov_id, inst_id) in preview.default_instances {
        if staged.get_provider_instance(&inst_id).is_some() {
            staged.set_default_instance_for_provider(&prov_id, &inst_id)?;
        }
    }

    result.added_count = result.added.len();
    result.updated_count = result.updated.len();
    result.skipped_count = result.skipped.len();

    // Commit changes atomically
    *catalog = staged;
    Ok(result)
}

// ----------------------------------------------------------------------------
// Internal Helpers
// ----------------------------------------------------------------------------

/// Categorizes incoming instances into added, updated, skipped, and conflicts.
fn categorize_instances(
    catalog: &ProviderCatalog,
    instances: Vec<ProviderInstance>,
    strategy: ConflictResolutionStrategy,
    preview: &mut ImportPreview,
) -> Result<(), ProviderError> {
    let mut seen_ids = HashSet::new();

    for incoming in instances {
        incoming.validate()?;

        // Prevent intra-batch ID collisions
        if !seen_ids.insert(incoming.id) {
            return Err(ProviderError::DuplicateProviderInstance(incoming.id));
        }

        if let Some(existing) = find_existing_instance(catalog, &incoming) {
            let reason = if existing.id == incoming.id {
                format!(
                    "Provider instance ID '{}' already exists in catalog",
                    incoming.id
                )
            } else {
                format!(
                    "Provider instance with display name '{}' already exists for provider '{}'",
                    incoming.display_name, incoming.provider_id
                )
            };

            let conflict = ImportConflict {
                instance_id: existing.id,
                existing_name: existing.display_name.clone(),
                incoming_name: incoming.display_name.clone(),
                reason,
            };
            preview.conflicts.push(conflict);

            match strategy {
                ConflictResolutionStrategy::Overwrite => {
                    let mut updated_instance = incoming.clone();
                    // Preserve existing instance ID to maintain referential integrity with models
                    updated_instance.id = existing.id;
                    preview.updated.push(updated_instance);
                }
                ConflictResolutionStrategy::Skip => {
                    preview.skipped.push(incoming);
                }
                ConflictResolutionStrategy::Error => {
                    // Conflict recorded; neither updated nor skipped
                }
            }
        } else {
            preview.added.push(incoming);
        }
    }

    Ok(())
}

/// Finds an existing instance in the catalog that conflicts with the incoming instance.
fn find_existing_instance<'a>(
    catalog: &'a ProviderCatalog,
    incoming: &ProviderInstance,
) -> Option<&'a ProviderInstance> {
    // Check ID match first
    if let Some(existing) = catalog.get_provider_instance(&incoming.id) {
        return Some(existing);
    }

    // Check match by provider family and display name (case-insensitive)
    catalog
        .get_instances_for_provider(&incoming.provider_id)
        .into_iter()
        .find(|existing| {
            existing
                .display_name
                .trim()
                .eq_ignore_ascii_case(incoming.display_name.trim())
        })
}

/// Parses a Codex TOML string into ProviderInstances.
fn parse_codex_toml(
    catalog: &ProviderCatalog,
    content: &str,
) -> Result<Vec<ProviderInstance>, ProviderError> {
    let root: toml::Table =
        toml::from_str(content).map_err(|e| ProviderError::ImportParseError(e.to_string()))?;

    let providers_table = if let Some(mp) = root.get("model_providers") {
        mp.as_table().ok_or_else(|| {
            ProviderError::ImportParseError(
                "'model_providers' key in TOML must be a table".to_string(),
            )
        })?
    } else {
        // Fall back to treating the root table as a map of provider sections if suitable
        &root
    };

    let mut instances = Vec::new();

    for (key, val) in providers_table {
        let entry_table = match val.as_table() {
            Some(t) => t,
            None => continue, // Skip non-table entries (e.g. scalar configuration keys)
        };

        // Friendly display name
        let name = entry_table
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| key.clone());

        // Base URL
        let base_url = entry_table
            .get("base_url")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or("https://api.openai.com/v1");

        let endpoint = EndpointProfile::new(base_url)?;

        // Environment variable for API key
        let env_key = entry_table
            .get("env_key")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        let authentication = match env_key {
            Some(k) => AuthenticationScheme::BearerToken {
                secret: SecretReference::env(k)?,
            },
            None => AuthenticationScheme::None,
        };

        // Wire API protocol
        let wire_api = entry_table
            .get("wire_api")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_else(|| "responses".to_string());

        let protocol = match wire_api.as_str() {
            "responses" => ProtocolFamily::OpenAiResponses,
            "chat" => ProtocolFamily::OpenAiChatCompletions,
            other => ProtocolFamily::Custom(other.to_string()),
        };

        // Resolve ProviderId
        let provider_id = resolve_codex_provider_id(catalog, key, entry_table)?;

        // Deterministic ProviderInstanceId derived from provider key
        let uuid = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("codex:model_provider:{key}").as_bytes(),
        );
        let id = ProviderInstanceId::from_uuid(uuid);

        let instance =
            ProviderInstance::new(id, provider_id, name, protocol, endpoint, authentication)?;
        instances.push(instance);
    }

    Ok(instances)
}

/// Resolves the ProviderId for a Codex provider configuration entry.
fn resolve_codex_provider_id(
    catalog: &ProviderCatalog,
    key: &str,
    table: &toml::Table,
) -> Result<ProviderId, ProviderError> {
    // 1. Explicit provider field in table
    if let Some(prov) = table.get("provider").or_else(|| table.get("provider_id"))
        && let Some(s) = prov.as_str()
    {
        return ProviderId::new(s.trim());
    }

    // 2. Exact match in catalog
    if let Ok(pid) = ProviderId::new(key)
        && catalog.get_provider_definition(&pid).is_some()
    {
        return Ok(pid);
    }

    // 3. Heuristic matching based on key prefix/name
    let lower_key = key.to_lowercase();
    if lower_key.contains("openai") {
        return ProviderId::new("openai");
    }
    if lower_key.contains("ollama") {
        return ProviderId::new("ollama");
    }
    if lower_key.contains("openrouter") {
        return ProviderId::new("openrouter");
    }
    if lower_key.contains("bedrock")
        && catalog
            .get_provider_definition(&ProviderId::new("amazon-bedrock").unwrap())
            .is_some()
    {
        return ProviderId::new("amazon-bedrock");
    }

    // 4. Default to openai or custom if present in catalog
    if catalog
        .get_provider_definition(&ProviderId::new("openai").unwrap())
        .is_some()
    {
        Ok(ProviderId::new("openai").unwrap())
    } else if catalog
        .get_provider_definition(&ProviderId::new("custom").unwrap())
        .is_some()
    {
        Ok(ProviderId::new("custom").unwrap())
    } else {
        ProviderId::new(key)
    }
}

/// Parses an OpenCode JSON configuration into ProviderInstances.
fn parse_opencode_json(
    catalog: &ProviderCatalog,
    content: &str,
) -> Result<Vec<ProviderInstance>, ProviderError> {
    let root: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| ProviderError::ImportParseError(e.to_string()))?;

    let root_obj = root.as_object().ok_or_else(|| {
        ProviderError::ImportParseError("OpenCode configuration must be a JSON object".to_string())
    })?;

    let providers_map = if let Some(p) = root_obj.get("providers") {
        p.as_object().ok_or_else(|| {
            ProviderError::ImportParseError(
                "'providers' in OpenCode JSON must be an object".to_string(),
            )
        })?
    } else {
        root_obj
    };

    let mut instances = Vec::new();

    for (key, val) in providers_map {
        let entry = match val.as_object() {
            Some(o) => o,
            None => continue,
        };

        // Display name
        let name = entry
            .get("name")
            .or_else(|| entry.get("displayName"))
            .or_else(|| entry.get("display_name"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| key.clone());

        // Provider ID
        let provider_id = resolve_opencode_provider_id(catalog, key, entry)?;

        // Protocol
        let protocol = resolve_opencode_protocol(&provider_id, entry)?;

        // Base URL
        let base_url = entry
            .get("baseURL")
            .or_else(|| entry.get("baseUrl"))
            .or_else(|| entry.get("base_url"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| default_base_url_for_provider(&provider_id));

        let endpoint = EndpointProfile::new(base_url)?;

        // API Key reference
        let env_key = extract_opencode_api_key_env(entry);

        let authentication = match env_key {
            Some(var) => {
                let sec_ref = SecretReference::env(&var)?;
                if provider_id.as_str() == "anthropic" {
                    AuthenticationScheme::ApiKeyHeader {
                        header_name: "x-api-key".to_string(),
                        secret: sec_ref,
                    }
                } else {
                    AuthenticationScheme::BearerToken { secret: sec_ref }
                }
            }
            None => AuthenticationScheme::None,
        };

        // Deterministic ID
        let uuid = Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("opencode:provider:{key}").as_bytes(),
        );
        let id = ProviderInstanceId::from_uuid(uuid);

        let instance =
            ProviderInstance::new(id, provider_id, name, protocol, endpoint, authentication)?;
        instances.push(instance);
    }

    Ok(instances)
}

/// Resolves the ProviderId for an OpenCode provider entry.
fn resolve_opencode_provider_id(
    catalog: &ProviderCatalog,
    key: &str,
    entry: &serde_json::Map<String, serde_json::Value>,
) -> Result<ProviderId, ProviderError> {
    if let Some(prov) = entry.get("provider").or_else(|| entry.get("providerId"))
        && let Some(s) = prov.as_str()
    {
        return ProviderId::new(s.trim());
    }

    let lower = key.to_lowercase();
    match lower.as_str() {
        "anthropic" | "claude" => ProviderId::new("anthropic"),
        "openai" => ProviderId::new("openai"),
        "google" | "gemini" => {
            if catalog
                .get_provider_definition(&ProviderId::new("google").unwrap())
                .is_some()
            {
                ProviderId::new("google")
            } else {
                ProviderId::new("google-gemini")
            }
        }
        "openrouter" => ProviderId::new("openrouter"),
        "ollama" => ProviderId::new("ollama"),
        "lmstudio" | "lm-studio" => {
            if catalog
                .get_provider_definition(&ProviderId::new("lm-studio").unwrap())
                .is_some()
            {
                ProviderId::new("lm-studio")
            } else {
                ProviderId::new("lmstudio")
            }
        }
        _ => {
            if let Ok(pid) = ProviderId::new(key)
                && catalog.get_provider_definition(&pid).is_some()
            {
                return Ok(pid);
            }
            if catalog
                .get_provider_definition(&ProviderId::new("openai").unwrap())
                .is_some()
            {
                ProviderId::new("openai")
            } else {
                ProviderId::new(key)
            }
        }
    }
}

/// Resolves the ProtocolFamily for an OpenCode provider entry.
fn resolve_opencode_protocol(
    provider_id: &ProviderId,
    entry: &serde_json::Map<String, serde_json::Value>,
) -> Result<ProtocolFamily, ProviderError> {
    if let Some(proto) = entry.get("protocol").and_then(|v| v.as_str()) {
        let p_lower = proto.trim().to_lowercase();
        return match p_lower.as_str() {
            "anthropic" | "anthropic-messages" => Ok(ProtocolFamily::AnthropicMessages),
            "openai" | "openai-responses" | "responses" => Ok(ProtocolFamily::OpenAiResponses),
            "openai-chat" | "chat" => Ok(ProtocolFamily::OpenAiChatCompletions),
            "google" | "gemini" | "gemini-generate-content" => {
                Ok(ProtocolFamily::GeminiGenerateContent)
            }
            other => Ok(ProtocolFamily::Custom(other.to_string())),
        };
    }

    match provider_id.as_str() {
        "anthropic" => Ok(ProtocolFamily::AnthropicMessages),
        "google" | "google-gemini" => Ok(ProtocolFamily::GeminiGenerateContent),
        "openrouter" => Ok(ProtocolFamily::OpenAiChatCompletions),
        _ => Ok(ProtocolFamily::OpenAiResponses),
    }
}

/// Returns standard default base URL for well-known providers.
fn default_base_url_for_provider(provider_id: &ProviderId) -> &'static str {
    match provider_id.as_str() {
        "anthropic" => "https://api.anthropic.com",
        "google" | "google-gemini" => "https://generativelanguage.googleapis.com",
        "openrouter" => "https://openrouter.ai/api/v1",
        "ollama" => "http://localhost:11434",
        _ => "https://api.openai.com/v1",
    }
}

/// Extracts API key environment variable name from OpenCode JSON entry.
fn extract_opencode_api_key_env(
    entry: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let key_val = entry
        .get("apiKey")
        .or_else(|| entry.get("api_key"))
        .or_else(|| entry.get("apiKeyEnv"))
        .or_else(|| entry.get("envKey"))?;

    if let Some(s) = key_val.as_str() {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return None;
        }
        // Handle env:VAR and {env:VAR} prefixes
        let sanitized = if let Some(stripped) = trimmed.strip_prefix("env:") {
            stripped.trim()
        } else if let Some(stripped) = trimmed.strip_prefix("{env:") {
            stripped.trim_end_matches('}').trim()
        } else {
            trimmed
        };
        if sanitized.is_empty() {
            None
        } else {
            Some(sanitized.to_string())
        }
    } else if let Some(obj) = key_val.as_object() {
        obj.get("env")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    }
}
