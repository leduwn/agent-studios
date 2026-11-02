//! Cross-process single-writer database lock for Agent Studios daemon (M15-04.1).
//!
//! Provides mutual exclusion across distinct OS processes targeting the same SQLite database file.
//! Uses native OS kernel locking primitives (`LockFileEx` on Windows, `libc::flock` on Unix).
//!
//! Invariants:
//! 1. Resolves `<db_path>.lock` and verifies parent directory canonicalization to prevent path traversal/escape.
//! 2. Protects against symlink hijacking (both parent directory and lockfile).
//! 3. Distinguishes actively held OS kernel locks from stale lockfiles on disk.
//! 4. Held for daemon lifetime; released on RAII Drop or process termination.

use std::path::{Path, PathBuf};

use crate::error::TransportError;

/// Error type for database lock acquisition and validation operations.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("Database lockfile already held by another process: {0}")]
    AlreadyLocked(String),

    #[error("Lockfile path traversal or escape detected: {0}")]
    PathEscape(String),

    #[error("Lockfile path is a symlink: potential hijacking attack: {0}")]
    SymlinkForbidden(String),

    #[error("I/O error during lock operation: {0}")]
    Io(#[from] std::io::Error),

    #[error("Operating system error during lock operation: {0}")]
    Os(String),
}

impl From<LockError> for TransportError {
    fn from(err: LockError) -> Self {
        match err {
            LockError::AlreadyLocked(msg) => {
                TransportError::PermissionDenied(format!("Database locked: {msg}"))
            }
            LockError::PathEscape(msg) => {
                TransportError::PermissionDenied(format!("Path escape: {msg}"))
            }
            LockError::SymlinkForbidden(msg) => {
                TransportError::PermissionDenied(format!("Symlink forbidden: {msg}"))
            }
            LockError::Io(e) => TransportError::Io(e),
            LockError::Os(msg) => TransportError::Daemon(msg),
        }
    }
}

/// Resolves and validates the lockfile path `<db_path>.lock`.
/// Protects against directory traversal and symlink hijacking attacks.
pub fn resolve_lock_path(db_path: impl AsRef<Path>) -> Result<PathBuf, LockError> {
    let db_path = db_path.as_ref();

    let file_name = db_path
        .file_name()
        .ok_or_else(|| LockError::PathEscape("Database path has no file name".into()))?;

    let file_name_str = file_name
        .to_str()
        .ok_or_else(|| LockError::PathEscape("Database file name is not valid UTF-8".into()))?;

    if file_name_str.is_empty()
        || file_name_str == "."
        || file_name_str == ".."
        || file_name_str.contains('/')
        || file_name_str.contains('\\')
    {
        return Err(LockError::PathEscape(format!(
            "Database file name contains invalid traversal elements: {file_name_str}"
        )));
    }

    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        std::fs::create_dir_all(parent)?;
    }

    // Check if parent itself is a symlink
    if let Ok(meta) = std::fs::symlink_metadata(parent)
        && meta.file_type().is_symlink()
    {
        return Err(LockError::SymlinkForbidden(format!(
            "Database parent directory is a symlink: {}",
            parent.display()
        )));
    }

    // Canonicalize parent directory to prevent directory traversal and symlink redirection
    let canonical_parent = dunce::canonicalize(parent)
        .map_err(|e| LockError::PathEscape(format!("Failed to canonicalize parent dir: {e}")))?;

    let mut lock_name = file_name.to_os_string();
    lock_name.push(".lock");
    let lock_path = canonical_parent.join(lock_name);

    // Verify lock path did not escape the canonical parent
    if lock_path.parent() != Some(&canonical_parent) {
        return Err(LockError::PathEscape(format!(
            "Resolved lockfile path {} escaped parent {}",
            lock_path.display(),
            canonical_parent.display()
        )));
    }

    // If lockfile already exists on disk, ensure it is NOT a symlink
    if let Ok(meta) = std::fs::symlink_metadata(&lock_path)
        && meta.file_type().is_symlink()
    {
        return Err(LockError::SymlinkForbidden(format!(
            "Lockfile {} is a symlink (potential hijacking attack)",
            lock_path.display()
        )));
    }

    Ok(lock_path)
}

/// Cross-process mutual exclusion lock on a database file.
pub struct DatabaseLock {
    lock_path: PathBuf,
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
    #[cfg(unix)]
    file: std::fs::File,
}

#[cfg(windows)]
unsafe impl Send for DatabaseLock {}
#[cfg(windows)]
unsafe impl Sync for DatabaseLock {}

impl std::fmt::Debug for DatabaseLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseLock")
            .field("lock_path", &self.lock_path)
            .finish()
    }
}

impl DatabaseLock {
    /// Attempts to acquire an exclusive cross-process lock on the database lockfile `<db_path>.lock`.
    /// Returns `Err(LockError::AlreadyLocked(...))` if another OS process actively holds the lock.
    /// A stale lockfile left on disk from a prior ungraceful crash will NOT prevent acquisition.
    pub fn acquire(db_path: impl AsRef<Path>) -> Result<Self, LockError> {
        let lock_path = resolve_lock_path(db_path)?;

        #[cfg(windows)]
        {
            Self::acquire_windows(lock_path)
        }

        #[cfg(unix)]
        {
            Self::acquire_unix(lock_path)
        }

        #[cfg(not(any(windows, unix)))]
        {
            Err(LockError::Os(
                "Unsupported operating system for DatabaseLock".into(),
            ))
        }
    }

    /// Returns the filesystem path of the held lockfile.
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }

    #[cfg(windows)]
    fn acquire_windows(lock_path: PathBuf) -> Result<Self, LockError> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{
            CloseHandle, ERROR_ACCESS_DENIED, ERROR_LOCK_VIOLATION, ERROR_SHARING_VIOLATION,
            GetLastError, INVALID_HANDLE_VALUE,
        };
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, LOCKFILE_EXCLUSIVE_LOCK,
            LOCKFILE_FAIL_IMMEDIATELY, LockFileEx, OPEN_ALWAYS,
        };
        use windows_sys::Win32::System::IO::OVERLAPPED;

        let mut wide_path: Vec<u16> = lock_path.as_os_str().encode_wide().collect();
        wide_path.push(0);

        // Open with shared read, write, and delete permissions so other processes can attempt
        // to open and query the lock rather than getting an opaque file-sharing denial.
        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            let err = unsafe { GetLastError() };
            if err == ERROR_SHARING_VIOLATION || err == ERROR_ACCESS_DENIED {
                return Err(LockError::AlreadyLocked(format!(
                    "Another process actively holds the database lock on {}",
                    lock_path.display()
                )));
            }
            return Err(LockError::Os(format!(
                "CreateFileW failed for {}: OS error code {err}",
                lock_path.display()
            )));
        }

        // Apply kernel byte-range lock via LockFileEx with fail-immediately semantics
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        let lock_flags = LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY;
        let lock_success = unsafe {
            LockFileEx(
                handle,
                lock_flags,
                0, // dwReserved
                1, // nNumberOfBytesToLockLow
                0, // nNumberOfBytesToLockHigh
                &mut overlapped,
            )
        };

        if lock_success == 0 {
            let err = unsafe { GetLastError() };
            unsafe { CloseHandle(handle) };
            if err == ERROR_LOCK_VIOLATION
                || err == ERROR_SHARING_VIOLATION
                || err == ERROR_ACCESS_DENIED
            {
                return Err(LockError::AlreadyLocked(format!(
                    "Another process actively holds the database lock on {}",
                    lock_path.display()
                )));
            }
            return Err(LockError::Os(format!(
                "LockFileEx failed for {}: OS error code {err}",
                lock_path.display()
            )));
        }

        // Write PID for diagnostic readability
        let pid_info = format!(
            "pid={}\ntimestamp={}\n",
            std::process::id(),
            chrono::Utc::now().to_rfc3339()
        );
        let mut written: u32 = 0;
        unsafe {
            use windows_sys::Win32::Storage::FileSystem::WriteFile;
            WriteFile(
                handle,
                pid_info.as_ptr(),
                pid_info.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            );
        }

        Ok(Self { lock_path, handle })
    }

    #[cfg(unix)]
    fn acquire_unix(lock_path: PathBuf) -> Result<Self, LockError> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::io::AsRawFd;

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&lock_path)
            .map_err(LockError::Io)?;

        let fd = file.as_raw_fd();
        let res = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        if res != 0 {
            let errno = std::io::Error::last_os_error();
            if errno.raw_os_error() == Some(libc::EWOULDBLOCK)
                || errno.raw_os_error() == Some(libc::EAGAIN)
            {
                return Err(LockError::AlreadyLocked(format!(
                    "Another process actively holds the database lock on {}",
                    lock_path.display()
                )));
            }
            return Err(LockError::Os(format!(
                "libc::flock failed for {}: {errno}",
                lock_path.display()
            )));
        }

        let _ = file.set_len(0);
        let pid_info = format!(
            "pid={}\ntimestamp={}\n",
            std::process::id(),
            chrono::Utc::now().to_rfc3339()
        );
        let _ = file.write_all(pid_info.as_bytes());
        let _ = file.flush();

        Ok(Self { lock_path, file })
    }
}

#[cfg(windows)]
impl Drop for DatabaseLock {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::UnlockFileEx;
        use windows_sys::Win32::System::IO::OVERLAPPED;

        if self.handle != INVALID_HANDLE_VALUE {
            let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
            unsafe {
                UnlockFileEx(self.handle, 0, 1, 0, &mut overlapped);
                CloseHandle(self.handle);
            }
            self.handle = INVALID_HANDLE_VALUE;
        }
    }
}

#[cfg(unix)]
impl Drop for DatabaseLock {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;
        let fd = self.file.as_raw_fd();
        unsafe {
            libc::flock(fd, libc::LOCK_UN);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_lock_acquire_and_drop_cycle() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_lock.db");

        // 1. Initial acquisition must succeed
        let lock1 = DatabaseLock::acquire(&db_path).expect("Failed to acquire lock");
        assert!(lock1.lock_path().exists());
        assert_eq!(lock1.lock_path(), resolve_lock_path(&db_path).unwrap());

        // 2. Concurrent acquisition while held must fail with AlreadyLocked
        let lock2_res = DatabaseLock::acquire(&db_path);
        assert!(matches!(lock2_res, Err(LockError::AlreadyLocked(_))));

        // 3. Drop first lock
        drop(lock1);

        // 4. Stale lockfile remains on disk, but re-acquisition must now succeed immediately
        assert!(resolve_lock_path(&db_path).unwrap().exists());
        let lock3 =
            DatabaseLock::acquire(&db_path).expect("Re-acquisition of stale lockfile must succeed");
        assert!(lock3.lock_path().exists());

        // 5. Concurrent acquisition still fails while lock3 is held
        assert!(matches!(
            DatabaseLock::acquire(&db_path),
            Err(LockError::AlreadyLocked(_))
        ));

        drop(lock3);
    }

    #[test]
    fn test_lock_path_resolution_and_escape_checks() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("data.db");
        let resolved = resolve_lock_path(&db_path).unwrap();
        assert_eq!(resolved.file_name().unwrap(), "data.db.lock");

        // Empty path has no file name
        let invalid = Path::new("");
        assert!(resolve_lock_path(invalid).is_err());
    }
}
