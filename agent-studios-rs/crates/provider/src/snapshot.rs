use serde::{Deserialize, Serialize};

use crate::catalog::ProviderCatalog;
use crate::definition::ProviderDefinition;
use crate::error::ProviderError;
use crate::instance::ProviderInstance;
use crate::model::ModelDescriptor;

/// Schema version for exported ProviderCatalog snapshots.
pub const PROVIDER_CATALOG_SCHEMA_VERSION: u32 = 1;

/// Exportable snapshot of the complete provider and model catalog.
///
/// Contains NO raw credentials. All secrets are stored as opaque SecretReferences.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCatalogSnapshot {
    pub schema_version: u32,
    pub definitions: Vec<ProviderDefinition>,
    pub instances: Vec<ProviderInstance>,
    pub models: Vec<ModelDescriptor>,
}

impl ProviderCatalog {
    /// Exports the current catalog state into a deterministic snapshot.
    pub fn export_snapshot(&self) -> ProviderCatalogSnapshot {
        let definitions = self
            .list_provider_definitions()
            .into_iter()
            .cloned()
            .collect();
        let instances = self
            .list_provider_instances()
            .into_iter()
            .cloned()
            .collect();
        let models = self.list_models().into_iter().cloned().collect();

        ProviderCatalogSnapshot {
            schema_version: PROVIDER_CATALOG_SCHEMA_VERSION,
            definitions,
            instances,
            models,
        }
    }

    /// Creates a new ProviderCatalog from a snapshot using strict staged validation.
    ///
    /// Validates all schema versions, definitions, instances, and cross-references.
    /// If any validation check fails, returns an error without constructing a catalog.
    pub fn from_snapshot(snapshot: ProviderCatalogSnapshot) -> Result<Self, ProviderError> {
        if snapshot.schema_version != PROVIDER_CATALOG_SCHEMA_VERSION {
            return Err(ProviderError::UnsupportedSchemaVersion {
                version: snapshot.schema_version,
                supported: PROVIDER_CATALOG_SCHEMA_VERSION,
            });
        }

        let mut staged = Self::new();

        for def in snapshot.definitions {
            staged.register_provider_definition(def)?;
        }

        for inst in snapshot.instances {
            staged.register_provider_instance(inst)?;
        }

        for model in snapshot.models {
            staged.register_model(model)?;
        }

        Ok(staged)
    }

    /// Imports a snapshot into the current catalog using strict atomic/staged validation.
    ///
    /// The entire snapshot is validated into a temporary catalog first.
    /// If validation fails at any point, the existing catalog remains completely untouched.
    pub fn import_snapshot(
        &mut self,
        snapshot: ProviderCatalogSnapshot,
    ) -> Result<(), ProviderError> {
        let staged = Self::from_snapshot(snapshot)?;
        *self = staged;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthenticationScheme;
    use crate::capabilities::ModelCapabilities;
    use crate::endpoint::EndpointProfile;
    use crate::id::{ModelId, ProviderId, ProviderInstanceId};
    use crate::model::ModelLimits;
    use crate::protocol::ProtocolFamily;
    use crate::secret::SecretReference;

    #[test]
    fn test_snapshot_roundtrip() {
        let mut catalog = ProviderCatalog::new();
        let prov_id = ProviderId::new("openai").unwrap();
        let def = ProviderDefinition::new(
            prov_id.clone(),
            "OpenAI",
            vec![ProtocolFamily::OpenAiResponses],
        )
        .unwrap();
        catalog.register_provider_definition(def).unwrap();

        let inst_id = ProviderInstanceId::new();
        let ep = EndpointProfile::new("https://api.openai.com/v1").unwrap();
        let auth = AuthenticationScheme::BearerToken {
            secret: SecretReference::env("OPENAI_KEY").unwrap(),
        };
        let inst = ProviderInstance::new(
            inst_id,
            prov_id,
            "Personal",
            ProtocolFamily::OpenAiResponses,
            ep,
            auth,
        )
        .unwrap();
        catalog.register_provider_instance(inst).unwrap();

        let m_id = ModelId::new("gpt-5.6").unwrap();
        let desc = ModelDescriptor::new(
            inst_id,
            m_id,
            "GPT 5.6",
            ModelCapabilities::unknown(),
            ModelLimits::default(),
        )
        .unwrap();
        catalog.register_model(desc).unwrap();

        let snapshot = catalog.export_snapshot();
        assert_eq!(snapshot.schema_version, PROVIDER_CATALOG_SCHEMA_VERSION);
        assert_eq!(snapshot.definitions.len(), 1);
        assert_eq!(snapshot.instances.len(), 1);
        assert_eq!(snapshot.models.len(), 1);

        // Serialize snapshot to JSON
        let json = serde_json::to_string_pretty(&snapshot).unwrap();

        // Deserialize back
        let de_snapshot: ProviderCatalogSnapshot = serde_json::from_str(&json).unwrap();
        let imported = ProviderCatalog::from_snapshot(de_snapshot).unwrap();
        assert_eq!(catalog, imported);
    }

    #[test]
    fn test_snapshot_atomic_rollback_on_error() {
        let mut catalog = ProviderCatalog::new();
        let prov_id = ProviderId::new("openai").unwrap();
        let def = ProviderDefinition::new(
            prov_id.clone(),
            "OpenAI",
            vec![ProtocolFamily::OpenAiResponses],
        )
        .unwrap();
        catalog.register_provider_definition(def).unwrap();

        // Create invalid snapshot with orphaned model
        let orphaned_model = ModelDescriptor::new(
            ProviderInstanceId::new(), // non-existent instance
            ModelId::new("orphan").unwrap(),
            "Orphan",
            ModelCapabilities::unknown(),
            ModelLimits::default(),
        )
        .unwrap();

        let bad_snapshot = ProviderCatalogSnapshot {
            schema_version: PROVIDER_CATALOG_SCHEMA_VERSION,
            definitions: vec![],
            instances: vec![],
            models: vec![orphaned_model],
        };

        let err = catalog.import_snapshot(bad_snapshot).unwrap_err();
        assert!(matches!(err, ProviderError::UnknownProviderInstance(_)));

        // Verify catalog untouched
        assert_eq!(catalog.list_provider_definitions().len(), 1);
        assert!(catalog.get_provider_definition(&prov_id).is_some());
    }

    #[test]
    fn test_snapshot_unsupported_version_rejected() {
        let bad_version = ProviderCatalogSnapshot {
            schema_version: 999,
            definitions: vec![],
            instances: vec![],
            models: vec![],
        };
        let err = ProviderCatalog::from_snapshot(bad_version).unwrap_err();
        assert_eq!(
            err,
            ProviderError::UnsupportedSchemaVersion {
                version: 999,
                supported: PROVIDER_CATALOG_SCHEMA_VERSION
            }
        );
    }
}
