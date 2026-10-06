use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use agent_studios_protocol::artifact::{ArtifactKind, ArtifactRecord};
use agent_studios_protocol::id::{AgentId, ArtifactId, RunId, StudioId, TaskId, WorktreeId};

use crate::error::ArtifactStoreError;

/// Optional parameters for storing a versioned artifact.
#[derive(Clone, Debug, Default)]
pub struct ArtifactOptions {
    pub run_id: Option<RunId>,
    pub worktree_id: Option<WorktreeId>,
    pub version: u32,
    pub supersedes: Option<ArtifactId>,
    pub relative_path: Option<String>,
}

impl ArtifactOptions {
    pub fn new() -> Self {
        Self {
            version: 1,
            ..Default::default()
        }
    }

    pub fn with_run_id(mut self, run_id: Option<RunId>) -> Self {
        self.run_id = run_id;
        self
    }

    pub fn with_worktree_id(mut self, worktree_id: Option<WorktreeId>) -> Self {
        self.worktree_id = worktree_id;
        self
    }

    pub fn with_version(mut self, version: u32) -> Self {
        self.version = version;
        self
    }

    pub fn with_supersedes(mut self, supersedes: Option<ArtifactId>) -> Self {
        self.supersedes = supersedes;
        self
    }

    pub fn with_relative_path(mut self, relative_path: Option<String>) -> Self {
        self.relative_path = relative_path;
        self
    }
}

/// Content-addressed local artifact store.
/// Stores blobs keyed by SHA-256 hash under `<artifact_root>/blobs/sha256/<hash>`.
#[derive(Clone, Debug)]
pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, ArtifactStoreError> {
        let root = root.into();
        let blobs_dir = root.join("blobs").join("sha256");
        let tmp_dir = root.join("tmp");
        fs::create_dir_all(&blobs_dir)?;
        fs::create_dir_all(&tmp_dir)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Computes the SHA-256 hex digest of the given data.
    pub fn compute_hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    /// Stores raw data into the content-addressed blob store atomically.
    /// Returns `(content_hash, size_bytes)`.
    pub fn store_bytes(&self, data: &[u8]) -> Result<(String, usize), ArtifactStoreError> {
        let hash = Self::compute_hash(data);
        let blob_dir = self.root.join("blobs").join("sha256");
        let dest_path = blob_dir.join(&hash);

        // Deduplication: if blob already exists, skip writing.
        if dest_path.exists() {
            return Ok((hash, data.len()));
        }

        let tmp_dir = self.root.join("tmp");
        fs::create_dir_all(&tmp_dir)?;
        let tmp_path = tmp_dir.join(format!("{}.tmp", Uuid::new_v4()));

        {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(data)?;
            file.flush()?;
        }

        // Atomic move into destination path.
        match fs::rename(&tmp_path, &dest_path) {
            Ok(_) => Ok((hash, data.len())),
            Err(_) if dest_path.exists() => {
                // Another thread/process wrote the same blob concurrently. Clean up temp.
                let _ = fs::remove_file(&tmp_path);
                Ok((hash, data.len()))
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp_path);
                Err(ArtifactStoreError::Io(e))
            }
        }
    }

    /// Reads raw data for a given SHA-256 hex digest.
    pub fn read_bytes(&self, content_hash: &str) -> Result<Vec<u8>, ArtifactStoreError> {
        let hash = content_hash.strip_prefix("sha256:").unwrap_or(content_hash);
        let blob_path = self.root.join("blobs").join("sha256").join(hash);
        if !blob_path.exists() {
            return Err(ArtifactStoreError::BlobNotFound(hash.to_string()));
        }

        let data = fs::read(&blob_path)?;
        let actual_hash = Self::compute_hash(&data);
        if actual_hash != hash {
            return Err(ArtifactStoreError::HashMismatch {
                expected: hash.to_string(),
                actual: actual_hash,
            });
        }

        Ok(data)
    }

    /// Checks if a blob exists for the given content hash.
    pub fn has_blob(&self, content_hash: &str) -> bool {
        let hash = content_hash.strip_prefix("sha256:").unwrap_or(content_hash);
        self.root.join("blobs").join("sha256").join(hash).is_file()
    }

    /// Returns the filesystem path to a blob by hash.
    pub fn blob_path(&self, content_hash: &str) -> PathBuf {
        let hash = content_hash.strip_prefix("sha256:").unwrap_or(content_hash);
        self.root.join("blobs").join("sha256").join(hash)
    }

    /// High-level helper: stores raw data into the store and creates a fully populated `ArtifactRecord`.
    pub fn store_artifact(
        &self,
        studio_id: StudioId,
        task_id: TaskId,
        producer_agent_id: AgentId,
        kind: ArtifactKind,
        logical_name: impl Into<String>,
        data: &[u8],
        options: ArtifactOptions,
    ) -> Result<ArtifactRecord, ArtifactStoreError> {
        let (hash, size) = self.store_bytes(data)?;
        let location = format!("blobs/sha256/{}", hash);
        let now = Utc::now();

        let mut record = ArtifactRecord::new(
            studio_id,
            task_id,
            producer_agent_id,
            kind,
            logical_name,
            Some(format!("sha256:{}", hash)),
            location,
            now,
        );

        record.run_id = options.run_id;
        record.worktree_id = options.worktree_id;
        record.version = if options.version == 0 { 1 } else { options.version };
        record.supersedes = options.supersedes;
        record.relative_path = options.relative_path;
        record.size_bytes = Some(size as u64);

        Ok(record)
    }
}
