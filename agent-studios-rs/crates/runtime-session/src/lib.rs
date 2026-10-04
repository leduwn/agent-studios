//! Agent Studios Runtime Session
//!
//! Assembles configured providers, zero-discovery static model managers,
//! protocol drivers, and transport routers into unified `ModelRuntimeOverride` handles
//! and injects them directly into Codex thread sessions.

pub mod error;
pub mod factory;
pub mod models_manager;
pub mod prepared;

pub use error::RuntimeSessionError;
pub use factory::AgentStudiosRuntimeSessionFactory;
pub use models_manager::{StaticModelsManager, model_descriptor_to_model_info};
pub use prepared::PreparedRuntimeSession;
