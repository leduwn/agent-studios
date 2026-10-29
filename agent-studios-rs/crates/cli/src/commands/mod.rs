//! CLI Subcommand Handlers

pub mod model;
pub mod provider;

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::commands::model::{
        AddModelArgs, GetModelArgs, ListModelArgs, ModelCommands, OutputFormat as ModelOutputFormat,
        RemoveModelArgs, handle_model_command,
    };
    use crate::commands::provider::{
        AddProviderArgs, AuthTypeArg, GetProviderArgs, ImportProviderArgs, ListProviderArgs,
        OutputFormat as ProviderOutputFormat, ProviderCommands, RemoveProviderArgs,
        handle_provider_command,
    };
    use crate::config::{ENV_TEST_LOCK, load_catalog};

    #[tokio::test]
    async fn test_provider_and_model_cli_commands_lifecycle() {
        let _guard = ENV_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let rand_id = agent_studios_provider::id::ProviderInstanceId::new();
        let temp_dir = std::env::temp_dir().join(format!("ags_cli_test_{rand_id}"));
        unsafe {
            std::env::set_var("AGENT_STUDIOS_HOME", &temp_dir);
        }

        // 1. Initial list (empty or builtins)
        handle_provider_command(ProviderCommands::List(ListProviderArgs {
            format: ProviderOutputFormat::Json,
            family: None,
        }))
        .await
        .unwrap();

        // 2. Add provider
        handle_provider_command(ProviderCommands::Add(AddProviderArgs {
            provider_id: "openai".to_string(),
            name: "OpenAI Work".to_string(),
            endpoint: "https://api.openai.com/v1".to_string(),
            protocol: None,
            auth_type: AuthTypeArg::Bearer,
            secret_ref: Some("OPENAI_API_KEY".to_string()),
            header_name: "x-api-key".to_string(),
            default: true,
        }))
        .await
        .unwrap();

        let catalog = load_catalog().unwrap();
        let instances = catalog.list_provider_instances();
        assert_eq!(instances.len(), 1);
        let inst_id = instances[0].id.to_string();

        // 3. Get provider
        handle_provider_command(ProviderCommands::Get(GetProviderArgs {
            instance_id: inst_id.clone(),
            format: ProviderOutputFormat::Json,
        }))
        .await
        .unwrap();

        // 4. Add model under instance
        handle_model_command(ModelCommands::Add(AddModelArgs {
            instance_id: inst_id.clone(),
            model_id: "gpt-4o".to_string(),
            name: Some("GPT-4o Omnimodel".to_string()),
            context_window: Some(128_000),
            max_output: Some(16_384),
            tools: true,
            vision: true,
        }))
        .await
        .unwrap();

        // 5. List models
        handle_model_command(ModelCommands::List(ListModelArgs {
            format: ModelOutputFormat::Json,
            instance: Some(inst_id.clone()),
            provider: None,
        }))
        .await
        .unwrap();

        // 6. Get model
        handle_model_command(ModelCommands::Get(GetModelArgs {
            model_ref: format!("{inst_id}/gpt-4o"),
            format: ModelOutputFormat::Json,
        }))
        .await
        .unwrap();

        // 7. Remove model
        handle_model_command(ModelCommands::Remove(RemoveModelArgs {
            instance_id: inst_id.clone(),
            model_id: "gpt-4o".to_string(),
        }))
        .await
        .unwrap();

        // 8. Remove provider
        handle_provider_command(ProviderCommands::Remove(RemoveProviderArgs {
            instance_id: inst_id.clone(),
            cascade: false,
        }))
        .await
        .unwrap();

        let after_removal = load_catalog().unwrap();
        assert!(after_removal.get_provider_instance(&instances[0].id).is_none());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
        unsafe {
            std::env::remove_var("AGENT_STUDIOS_HOME");
        }
    }

    #[tokio::test]
    async fn test_provider_import_cli_dry_run_and_commit() {
        let _guard = ENV_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let rand_id = agent_studios_provider::id::ProviderInstanceId::new();
        let temp_dir = std::env::temp_dir().join(format!("ags_cli_import_{rand_id}"));
        fs::create_dir_all(&temp_dir).unwrap();
        unsafe {
            std::env::set_var("AGENT_STUDIOS_HOME", &temp_dir);
        }

        let codex_toml_path = temp_dir.join("config.toml");
        let toml_content = r#"
model_provider = "custom-openai"

[model_providers.custom-openai]
name = "OpenAI Codex"
base_url = "https://api.openai.com/v1"
wire_specification = "responses"
"#;
        fs::write(&codex_toml_path, toml_content).unwrap();

        // Dry run
        handle_provider_command(ProviderCommands::Import(ImportProviderArgs {
            path: codex_toml_path.clone(),
            source: Some("codex".to_string()),
            strategy: "overwrite".to_string(),
            dry_run: true,
        }))
        .await
        .unwrap();

        let catalog_before = load_catalog().unwrap();
        assert_eq!(catalog_before.list_provider_instances().len(), 0);

        // Commit import
        handle_provider_command(ProviderCommands::Import(ImportProviderArgs {
            path: codex_toml_path,
            source: Some("codex".to_string()),
            strategy: "overwrite".to_string(),
            dry_run: false,
        }))
        .await
        .unwrap();

        let catalog_after = load_catalog().unwrap();
        assert_eq!(catalog_after.list_provider_instances().len(), 1);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
        unsafe {
            std::env::remove_var("AGENT_STUDIOS_HOME");
        }
    }
}
