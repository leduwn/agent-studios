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

    /// Validates that a content hash is strictly 64 lowercase hexadecimal characters [0-9a-f]{64},
    /// preventing path traversal, character injection, and directory escaping.
    pub fn validate_hash(content_hash: &str) -> Result<&str, ArtifactStoreError> {
        let hash = content_hash.strip_prefix("sha256:").unwrap_or(content_hash);
        if hash.len() != 64 || !hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err(ArtifactStoreError::InvalidPath(format!(
                "Invalid content hash: must be strictly 64 lowercase hex characters [0-9a-f]{{64}}, got {:?}",
                content_hash
            )));
        }
        Ok(hash)
    }

    /// Stores raw data into the content-addressed blob store atomically.
    /// Returns `(content_hash, size_bytes)`.
    pub fn store_bytes(&self, data: &[u8]) -> Result<(String, usize), ArtifactStoreError> {
        let hash = Self::compute_hash(data);
        Self::validate_hash(&hash)?;

        let blob_dir = self.root.join("blobs").join("sha256");
        let dest_path = blob_dir.join(&hash);

        // Deduplication: if blob already exists, check symlink safety and skip writing.
        if dest_path.exists() {
            let meta = fs::symlink_metadata(&dest_path)?;
            if meta.file_type().is_symlink() || !meta.is_file() {
                return Err(ArtifactStoreError::InvalidPath(format!(
                    "Existing blob destination is a symlink or not a regular file: {}",
                    dest_path.display()
                )));
            }
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
            Ok(_) => {
                let canonical_dest = dunce::canonicalize(&dest_path)?;
                let canonical_root = dunce::canonicalize(&self.root)?;
                if !canonical_dest.starts_with(&canonical_root) {
                    let _ = fs::remove_file(&dest_path);
                    return Err(ArtifactStoreError::InvalidPath(format!(
                        "Written blob {} escapes root {}",
                        canonical_dest.display(),
                        canonical_root.display()
                    )));
                }
                Ok((hash, data.len()))
            }
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
        let hash = Self::validate_hash(content_hash)?;
        let blob_path = self.root.join("blobs").join("sha256").join(hash);
        if !blob_path.exists() {
            return Err(ArtifactStoreError::BlobNotFound(hash.to_string()));
        }

        // Prevent symlink traversal
        let meta = fs::symlink_metadata(&blob_path)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(ArtifactStoreError::InvalidPath(format!(
                "Blob path is a symlink or not a regular file: {}",
                blob_path.display()
            )));
        }

        // Prevent escaping root directory
        let canonical_blob = dunce::canonicalize(&blob_path)?;
        let canonical_root = dunce::canonicalize(&self.root)?;
        if !canonical_blob.starts_with(&canonical_root) {
            return Err(ArtifactStoreError::InvalidPath(format!(
                "Blob path {} escapes root {}",
                canonical_blob.display(),
                canonical_root.display()
            )));
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
        let Ok(hash) = Self::validate_hash(content_hash) else {
            return false;
        };
        let blob_path = self.root.join("blobs").join("sha256").join(hash);
        if let Ok(meta) = fs::symlink_metadata(&blob_path) {
            meta.is_file() && !meta.file_type().is_symlink()
        } else {
            false
        }
    }

    /// Returns the filesystem path to a blob by hash.
    pub fn blob_path(&self, content_hash: &str) -> Result<PathBuf, ArtifactStoreError> {
        let hash = Self::validate_hash(content_hash)?;
        let blob_path = self.root.join("blobs").join("sha256").join(hash);
        if blob_path.exists() {
            let meta = fs::symlink_metadata(&blob_path)?;
            if meta.file_type().is_symlink() || !meta.is_file() {
                return Err(ArtifactStoreError::InvalidPath(format!(
                    "Blob path is a symlink or not a regular file: {}",
                    blob_path.display()
                )));
            }
        }
        Ok(blob_path)
    }

    /// High-level helper: stores raw data into the store and creates a fully populated `ArtifactRecord`.
    #[allow(clippy::too_many_arguments)]
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
        record.version = if options.version == 0 {
            1
        } else {
            options.version
        };
        record.supersedes = options.supersedes;
        record.relative_path = options.relative_path;
        record.size_bytes = Some(size as u64);

        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_validate_hash_strictly_lowercase_sha256() {
        // Valid 64-char lowercase hex
        let valid = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(ArtifactStore::validate_hash(valid).unwrap(), valid);

        // Valid with sha256: prefix
        let with_prefix = format!("sha256:{}", valid);
        assert_eq!(ArtifactStore::validate_hash(&with_prefix).unwrap(), valid);

        // Reject uppercase hex
        let upper = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";
        assert!(matches!(
            ArtifactStore::validate_hash(upper),
            Err(ArtifactStoreError::InvalidPath(_))
        ));

        // Reject mixed case hex
        let mixed = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852B855";
        assert!(matches!(
            ArtifactStore::validate_hash(mixed),
            Err(ArtifactStoreError::InvalidPath(_))
        ));

        // Reject path traversal
        assert!(matches!(
            ArtifactStore::validate_hash("../../../etc/passwd"),
            Err(ArtifactStoreError::InvalidPath(_))
        ));
        assert!(matches!(
            ArtifactStore::validate_hash("..\\..\\windows\\system32"),
            Err(ArtifactStoreError::InvalidPath(_))
        ));

        // Reject non-hex characters
        let invalid_chars = "g3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(matches!(
            ArtifactStore::validate_hash(invalid_chars),
            Err(ArtifactStoreError::InvalidPath(_))
        ));

        // Reject short / long hashes
        assert!(matches!(
            ArtifactStore::validate_hash("e3b0c442"),
            Err(ArtifactStoreError::InvalidPath(_))
        ));
        let too_long = format!("{}a", valid);
        assert!(matches!(
            ArtifactStore::validate_hash(&too_long),
            Err(ArtifactStoreError::InvalidPath(_))
        ));
    }

    #[test]
    fn test_artifact_store_symlink_rejection() {
        let dir = tempdir().unwrap();
        let store = ArtifactStore::new(dir.path()).unwrap();

        let data = b"symlink test payload";
        let (hash, _) = store.store_bytes(data).unwrap();

        // Blob should be readable normally
        let read = store.read_bytes(&hash).unwrap();
        assert_eq!(read, data);

        // Attempting to read a symlinked blob must be rejected
        let foreign_dir = tempdir().unwrap();
        let target_file = foreign_dir.path().join("secret.txt");
        fs::write(&target_file, b"top-secret").unwrap();

        let symlink_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let blob_dir = dir.path().join("blobs").join("sha256");
        let symlink_path = blob_dir.join(symlink_hash);

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target_file, &symlink_path).unwrap();
            let err = store.read_bytes(symlink_hash).unwrap_err();
            assert!(matches!(err, ArtifactStoreError::InvalidPath(_)));
            assert!(!store.has_blob(symlink_hash));
        }

        #[cfg(windows)]
        {
            // On Windows, if symlink creation succeeds (Developer Mode / elevated), test it.
            if std::os::windows::fs::symlink_file(&target_file, &symlink_path).is_ok() {
                let err = store.read_bytes(symlink_hash).unwrap_err();
                assert!(matches!(err, ArtifactStoreError::InvalidPath(_)));
                assert!(!store.has_blob(symlink_hash));
            }
        }
    }
}
