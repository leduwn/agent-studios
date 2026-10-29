//! Provider Management CLI Commands (`ags provider ...`)

use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;

use agent_studios_provider::auth::AuthenticationScheme;
use agent_studios_provider::endpoint::EndpointProfile;
use agent_studios_provider::id::{ProviderId, ProviderInstanceId};
use agent_studios_provider::importer::{
    ConflictResolutionStrategy, SourceFormat, apply_import, preview_import,
};
use agent_studios_provider::instance::ProviderInstance;
use agent_studios_provider::protocol::ProtocolFamily;
use agent_studios_provider::secret::SecretReference;
use agent_studios_provider::store::SecretStore;

use crate::config::{get_system_secret_store, load_catalog, save_catalog};

#[derive(Subcommand, Debug)]
pub enum ProviderCommands {
    /// List configured provider instances
    List(ListProviderArgs),

    /// Add a new provider instance
    Add(AddProviderArgs),

    /// Inspect a provider instance
    Get(GetProviderArgs),

    /// Remove a provider instance
    Remove(RemoveProviderArgs),

    /// Import provider configurations from Codex, OpenCode, or Agent Studios snapshots
    Import(ImportProviderArgs),

    /// Test connectivity and credentials for a provider instance
    Test(TestProviderArgs),
}

#[derive(Args, Debug)]
pub struct ListProviderArgs {
    /// Output format (table or json)
    #[arg(long, default_value = "table")]
    pub format: OutputFormat,

    /// Filter by provider family (e.g., openai, anthropic, ollama)
    #[arg(long)]
    pub family: Option<String>,
}

#[derive(Args, Debug)]
pub struct AddProviderArgs {
    /// Provider family ID (e.g. openai, anthropic, openrouter, ollama, custom)
    #[arg(value_name = "PROVIDER_ID")]
    pub provider_id: String,

    /// Human-friendly display name (e.g. "OpenAI Work", "Local Ollama")
    #[arg(long, short)]
    pub name: String,

    /// API endpoint base URL (e.g. "https://api.openai.com/v1", "http://localhost:11434")
    #[arg(long, short)]
    pub endpoint: String,

    /// Wire protocol family (e.g. openai-responses, openai-chat, anthropic-messages, gemini-generate-content)
    #[arg(long)]
    pub protocol: Option<String>,

    /// Authentication scheme type
    #[arg(long, default_value = "bearer", value_enum)]
    pub auth_type: AuthTypeArg,

    /// Secret reference (environment variable name like OPENAI_API_KEY, or os locator)
    #[arg(long)]
    pub secret_ref: Option<String>,

    /// Header name for api-key auth (defaults to x-api-key)
    #[arg(long, default_value = "x-api-key")]
    pub header_name: String,

    /// Mark this instance as the default for its provider family
    #[arg(long)]
    pub default: bool,
}

#[derive(Args, Debug)]
pub struct GetProviderArgs {
    /// Provider instance ID
    #[arg(value_name = "INSTANCE_ID")]
    pub instance_id: String,

    /// Output format (table or json)
    #[arg(long, default_value = "table")]
    pub format: OutputFormat,
}

#[derive(Args, Debug)]
pub struct RemoveProviderArgs {
    /// Provider instance ID
    #[arg(value_name = "INSTANCE_ID")]
    pub instance_id: String,

    /// Cascade removal to all associated models
    #[arg(long)]
    pub cascade: bool,
}

#[derive(Args, Debug)]
pub struct ImportProviderArgs {
    /// Path to configuration file (config.toml, open-code.json, or snapshot.json)
    #[arg(value_name = "PATH")]
    pub path: PathBuf,

    /// Explicit source format (codex, opencode, agent-studios)
    #[arg(long)]
    pub source: Option<String>,

    /// Conflict resolution strategy (overwrite, skip, error)
    #[arg(long, default_value = "overwrite")]
    pub strategy: String,

    /// Preview planned changes without committing to the catalog
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args, Debug)]
pub struct TestProviderArgs {
    /// Provider instance ID
    #[arg(value_name = "INSTANCE_ID")]
    pub instance_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum AuthTypeArg {
    None,
    Bearer,
    ApiKey,
}

#[derive(Serialize)]
struct ProviderRow<'a> {
    id: String,
    provider: &'a str,
    name: &'a str,
    protocol: String,
    endpoint: &'a str,
    auth: String,
    models: usize,
    is_default: bool,
}

pub async fn handle_provider_command(
    cmd: ProviderCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ProviderCommands::List(args) => handle_list(args),
        ProviderCommands::Add(args) => handle_add(args),
        ProviderCommands::Get(args) => handle_get(args),
        ProviderCommands::Remove(args) => handle_remove(args),
        ProviderCommands::Import(args) => handle_import(args),
        ProviderCommands::Test(args) => handle_test(args).await,
    }
}

fn handle_list(args: ListProviderArgs) -> Result<(), Box<dyn std::error::Error>> {
    let catalog = load_catalog()?;
    let filter_family = args.family.as_deref().map(|s| s.trim().to_lowercase());

    let instances: Vec<&ProviderInstance> = catalog
        .list_provider_instances()
        .into_iter()
        .filter(|inst| {
            if let Some(ref fam) = filter_family {
                inst.provider_id.as_str().to_lowercase() == *fam
            } else {
                true
            }
        })
        .collect();

    let rows: Vec<ProviderRow> = instances
        .iter()
        .map(|inst| {
            let model_count = catalog.models_for_provider_instance(&inst.id).len();
            let is_default = catalog
                .get_default_instance_id_for_provider(&inst.provider_id)
                .map(|def_id| def_id == &inst.id)
                .unwrap_or(false);

            let auth_summary = match &inst.authentication {
                AuthenticationScheme::None => "none".to_string(),
                AuthenticationScheme::BearerToken { secret } => {
                    format!("bearer({})", secret.backend)
                }
                AuthenticationScheme::ApiKeyHeader {
                    header_name,
                    secret,
                } => {
                    format!("{}({})", header_name, secret.backend)
                }
                AuthenticationScheme::QueryParameter {
                    parameter_name,
                    secret,
                } => {
                    format!("query:{}({})", parameter_name, secret.backend)
                }
                AuthenticationScheme::OAuthToken { secret } => {
                    format!("oauth({})", secret.backend)
                }
                AuthenticationScheme::AwsSigV4 {
                    service,
                    region,
                    credential,
                } => {
                    format!("awsv4:{}:{}({})", service, region, credential.backend)
                }
            };

            ProviderRow {
                id: inst.id.to_string(),
                provider: inst.provider_id.as_str(),
                name: &inst.display_name,
                protocol: inst.protocol.to_string(),
                endpoint: inst.endpoint.base_url.as_str(),
                auth: auth_summary,
                models: model_count,
                is_default,
            }
        })
        .collect();

    if args.format == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("No provider instances configured.");
        println!(
            "Use 'ags provider add <provider_id> --name <name> --endpoint <url>' or 'ags provider import <path>' to register one."
        );
        return Ok(());
    }

    // Print aligned table
    println!(
        "{:<38} {:<12} {:<20} {:<24} {:<18} {:<6} DEFAULT",
        "INSTANCE ID", "PROVIDER", "NAME", "PROTOCOL", "AUTH", "MODELS"
    );
    println!("{:-<128}", "");
    for r in &rows {
        let default_mark = if r.is_default { "★ YES" } else { "  no" };
        println!(
            "{:<38} {:<12} {:<20} {:<24} {:<18} {:<6} {}",
            r.id, r.provider, r.name, r.protocol, r.auth, r.models, default_mark
        );
    }
    println!("\nTotal: {} provider instance(s)", rows.len());
    Ok(())
}

fn handle_add(args: AddProviderArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut catalog = load_catalog()?;
    let provider_id = ProviderId::new(&args.provider_id)?;
    let endpoint = EndpointProfile::new(&args.endpoint)?;

    // Resolve protocol
    let protocol = if let Some(p) = args.protocol {
        ProtocolFamily::from_str(&p)?
    } else {
        match provider_id.as_str() {
            "anthropic" => ProtocolFamily::AnthropicMessages,
            "google" | "google-gemini" => ProtocolFamily::GeminiGenerateContent,
            "openrouter" => ProtocolFamily::OpenAiChatCompletions,
            _ => ProtocolFamily::OpenAiResponses,
        }
    };

    // Resolve authentication
    let authentication = match args.auth_type {
        AuthTypeArg::None => AuthenticationScheme::None,
        AuthTypeArg::Bearer => {
            let sec_str = args
                .secret_ref
                .ok_or("Authentication type 'bearer' requires --secret-ref <ENV_VAR_OR_LOCATOR>")?;
            let sec = if sec_str.starts_with("os:") || sec_str.contains("agent-studios:") {
                SecretReference::os_store(&sec_str)?
            } else {
                SecretReference::env(&sec_str)?
            };
            AuthenticationScheme::BearerToken { secret: sec }
        }
        AuthTypeArg::ApiKey => {
            let sec_str = args.secret_ref.ok_or(
                "Authentication type 'api-key' requires --secret-ref <ENV_VAR_OR_LOCATOR>",
            )?;
            let sec = if sec_str.starts_with("os:") || sec_str.contains("agent-studios:") {
                SecretReference::os_store(&sec_str)?
            } else {
                SecretReference::env(&sec_str)?
            };
            AuthenticationScheme::ApiKeyHeader {
                header_name: args.header_name,
                secret: sec,
            }
        }
    };

    let instance_id = ProviderInstanceId::new();
    let instance = ProviderInstance::new(
        instance_id,
        provider_id.clone(),
        args.name,
        protocol,
        endpoint,
        authentication,
    )?;

    catalog.register_provider_instance(instance)?;

    if args.default {
        catalog.set_default_instance_for_provider(&provider_id, &instance_id)?;
    }

    save_catalog(&catalog)?;
    println!(
        "✓ Successfully registered provider instance: {}",
        instance_id
    );
    if args.default {
        println!(
            "★ Marked as default instance for provider '{}'",
            provider_id
        );
    }
    Ok(())
}

fn handle_get(args: GetProviderArgs) -> Result<(), Box<dyn std::error::Error>> {
    let catalog = load_catalog()?;
    let id = ProviderInstanceId::from_str(&args.instance_id)?;
    let instance = catalog
        .get_provider_instance(&id)
        .ok_or_else(|| format!("Provider instance '{id}' not found in catalog"))?;

    let is_default = catalog
        .get_default_instance_id_for_provider(&instance.provider_id)
        .map(|def_id| def_id == &id)
        .unwrap_or(false);

    let models = catalog.models_for_provider_instance(&id);

    if args.format == OutputFormat::Json {
        let val = serde_json::json!({
            "id": instance.id,
            "provider_id": instance.provider_id,
            "name": instance.display_name,
            "protocol": instance.protocol,
            "endpoint": instance.endpoint.base_url,
            "authentication": instance.authentication,
            "is_default": is_default,
            "model_count": models.len(),
            "models": models.iter().map(|m| &m.id).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&val)?);
        return Ok(());
    }

    println!("Provider Instance Details");
    println!("{:-<50}", "");
    println!("  ID:          {}", instance.id);
    println!("  Family:      {}", instance.provider_id);
    println!("  Name:        {}", instance.display_name);
    println!("  Protocol:    {}", instance.protocol);
    println!("  Endpoint:    {}", instance.endpoint.base_url);
    println!("  Default:     {}", if is_default { "Yes" } else { "No" });
    match &instance.authentication {
        AuthenticationScheme::None => println!("  Auth:        None"),
        AuthenticationScheme::BearerToken { secret } => {
            println!("  Auth:        Bearer Token");
            println!("    Backend:   {}", secret.backend);
            println!("    Locator:   {}", secret.locator);
        }
        AuthenticationScheme::ApiKeyHeader {
            header_name,
            secret,
        } => {
            println!("  Auth:        Header '{}'", header_name);
            println!("    Backend:   {}", secret.backend);
            println!("    Locator:   {}", secret.locator);
        }
        AuthenticationScheme::QueryParameter {
            parameter_name,
            secret,
        } => {
            println!("  Auth:        Query Parameter '{}'", parameter_name);
            println!("    Backend:   {}", secret.backend);
            println!("    Locator:   {}", secret.locator);
        }
        AuthenticationScheme::OAuthToken { secret } => {
            println!("  Auth:        OAuth Token");
            println!("    Backend:   {}", secret.backend);
            println!("    Locator:   {}", secret.locator);
        }
        AuthenticationScheme::AwsSigV4 {
            service,
            region,
            credential,
        } => {
            println!("  Auth:        AWS SigV4 ({}:{})", service, region);
            println!("    Backend:   {}", credential.backend);
            println!("    Locator:   {}", credential.locator);
        }
    }
    println!("  Models:      {} model(s) registered", models.len());
    for m in models {
        println!("    - {}", m.id);
    }
    Ok(())
}

fn handle_remove(args: RemoveProviderArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut catalog = load_catalog()?;
    let id = ProviderInstanceId::from_str(&args.instance_id)?;

    if args.cascade {
        let (inst, removed_models) = catalog.remove_provider_instance_cascade(&id)?;
        save_catalog(&catalog)?;
        println!(
            "✓ Removed provider instance '{}' ({}) and {} associated model(s)",
            inst.display_name,
            id,
            removed_models.len()
        );
    } else {
        let inst = catalog.remove_provider_instance(&id)?;
        save_catalog(&catalog)?;
        println!(
            "✓ Removed provider instance '{}' ({})",
            inst.display_name, id
        );
    }
    Ok(())
}

fn handle_import(args: ImportProviderArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut catalog = load_catalog()?;
    let content = fs::read_to_string(&args.path)
        .map_err(|e| format!("Failed to read configuration file {:?}: {e}", args.path))?;

    let format = if let Some(ref s) = args.source {
        SourceFormat::from_str(s)?
    } else {
        // Auto-detect from file extension and content
        let ext = args
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext == "toml" {
            SourceFormat::Codex
        } else if content.contains("schema_version") && content.contains("definitions") {
            SourceFormat::AgentStudios
        } else {
            SourceFormat::OpenCode
        }
    };

    let strategy = ConflictResolutionStrategy::from_str(&args.strategy)?;
    let preview = preview_import(&catalog, format, &content, strategy)?;

    println!("Import Preview from {:?} (format: {})", args.path, format);
    println!("{:-<60}", "");
    println!("  Planned additions: {}", preview.added.len());
    for inst in &preview.added {
        println!(
            "    + [{}] {} ({})",
            inst.provider_id, inst.display_name, inst.id
        );
    }

    println!("  Planned updates:   {}", preview.updated.len());
    for inst in &preview.updated {
        println!(
            "    ~ [{}] {} ({})",
            inst.provider_id, inst.display_name, inst.id
        );
    }

    println!("  Planned skips:     {}", preview.skipped.len());
    for inst in &preview.skipped {
        println!(
            "    - [{}] {} ({})",
            inst.provider_id, inst.display_name, inst.id
        );
    }

    if !preview.conflicts.is_empty() {
        println!("  Conflicts detected: {}", preview.conflicts.len());
        for c in &preview.conflicts {
            println!("    ! Conflict on instance {}: {}", c.instance_id, c.reason);
        }
    }

    if args.dry_run {
        println!("\n[DRY RUN] No changes were written to the catalog.");
        return Ok(());
    }

    let result = apply_import(&mut catalog, preview, strategy)?;
    save_catalog(&catalog)?;
    println!(
        "\n✓ Import applied successfully: {} added, {} updated, {} skipped.",
        result.added_count, result.updated_count, result.skipped_count
    );
    Ok(())
}

async fn handle_test(args: TestProviderArgs) -> Result<(), Box<dyn std::error::Error>> {
    let catalog = load_catalog()?;
    let id = ProviderInstanceId::from_str(&args.instance_id)?;
    let instance = catalog
        .get_provider_instance(&id)
        .ok_or_else(|| format!("Provider instance '{id}' not found in catalog"))?;

    println!(
        "Testing provider instance '{}' ({})...",
        instance.display_name, instance.provider_id
    );
    println!("Endpoint: {}", instance.endpoint.base_url);

    // Validate credentials if required
    let secret_store = get_system_secret_store();
    let has_credential = match &instance.authentication {
        AuthenticationScheme::None => {
            println!("Auth: None (no credentials required)");
            true
        }
        AuthenticationScheme::BearerToken { secret } => {
            test_secret_resolution(&secret_store, secret, "Bearer token")
        }
        AuthenticationScheme::ApiKeyHeader {
            header_name,
            secret,
        } => test_secret_resolution(&secret_store, secret, &format!("Header '{header_name}'")),
        AuthenticationScheme::QueryParameter {
            parameter_name,
            secret,
        } => test_secret_resolution(
            &secret_store,
            secret,
            &format!("Query param '{parameter_name}'"),
        ),
        AuthenticationScheme::OAuthToken { secret } => {
            test_secret_resolution(&secret_store, secret, "OAuth token")
        }
        AuthenticationScheme::AwsSigV4 { credential, .. } => {
            test_secret_resolution(&secret_store, credential, "AWS SigV4 credential")
        }
    };

    if !has_credential {
        return Err("Credential verification failed. Ensure environment variable or OS keyring entry is configured.".into());
    }

    println!("✓ Provider instance validation succeeded.");
    Ok(())
}

fn test_secret_resolution(
    secret_store: &dyn SecretStore,
    secret: &SecretReference,
    label: &str,
) -> bool {
    print!(
        "Auth: {} via {} ({})... ",
        label, secret.backend, secret.locator
    );
    match secret_store.get_secret(secret) {
        Ok(val) => {
            let len = val.len();
            if len > 0 {
                println!("✓ Resolved ({} chars)", len);
                true
            } else {
                println!("✗ Empty secret value");
                false
            }
        }
        Err(e) => {
            println!("✗ Failed: {e}");
            false
        }
    }
}
