use crate::error::TransportError;
use std::path::{Path, PathBuf};

#[cfg(unix)]
pub fn get_current_uid() -> u32 {
    unsafe { libc::getuid() }
}

#[cfg(not(unix))]
pub fn get_current_uid() -> u32 {
    std::env::var("AGENT_STUDIOS_TEST_UID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000)
}

/// Validate directory mode and ownership.
/// Fails closed if UID does not match or if group/world permissions are granted.
pub fn validate_dir_mode_and_owner(
    mode: u32,
    dir_uid: u32,
    current_uid: u32,
) -> Result<(), TransportError> {
    if dir_uid != current_uid {
        return Err(TransportError::PermissionDenied(format!(
            "Directory owned by UID {}, expected current UID {}",
            dir_uid, current_uid
        )));
    }
    let perms = mode & 0o777;
    // Directory must not be group or world accessible (rwx for others forbidden)
    if (perms & 0o077) != 0 {
        return Err(TransportError::PermissionDenied(format!(
            "Directory has insecure permissions {:04o} (group or world accessible)",
            perms
        )));
    }
    // Owner must have rwx (0700)
    if (perms & 0o700) != 0o700 {
        return Err(TransportError::PermissionDenied(format!(
            "Directory permissions {:04o} missing owner rwx permissions (expected 0700)",
            perms
        )));
    }
    Ok(())
}

/// Validate socket file mode and ownership.
/// Fails closed if UID does not match or if group/world permissions are granted, or mode != 0600.
pub fn validate_socket_mode_and_owner(
    mode: u32,
    file_uid: u32,
    current_uid: u32,
) -> Result<(), TransportError> {
    if file_uid != current_uid {
        return Err(TransportError::PermissionDenied(format!(
            "Socket owned by UID {}, expected current UID {}",
            file_uid, current_uid
        )));
    }
    let perms = mode & 0o777;
    // Socket must not be group or world accessible
    if (perms & 0o077) != 0 {
        return Err(TransportError::PermissionDenied(format!(
            "Socket has insecure permissions {:04o} (group or world accessible)",
            perms
        )));
    }
    // Socket mode must strictly match 0600 (rw-------)
    if perms != 0o600 {
        return Err(TransportError::PermissionDenied(format!(
            "Socket permissions are {:04o}, expected 0600",
            perms
        )));
    }
    Ok(())
}

/// Validate directory permissions on filesystem.
pub fn validate_directory_permissions(path: &Path) -> Result<(), TransportError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(path).map_err(TransportError::Io)?;
        if !metadata.is_dir() {
            return Err(TransportError::PermissionDenied(format!(
                "Path {:?} is not a directory",
                path
            )));
        }
        let mode = metadata.mode();
        let dir_uid = metadata.uid();
        let current_uid = get_current_uid();
        validate_dir_mode_and_owner(mode, dir_uid, current_uid)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// Ensure directory exists with 0700 permissions.
fn create_secure_dir(dir: &Path) -> Result<(), TransportError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        if !dir.exists() {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            builder.mode(0o700);
            builder.create(dir).map_err(TransportError::Io)?;
        }
        let metadata = std::fs::metadata(dir).map_err(TransportError::Io)?;
        let mut perms = metadata.permissions();
        perms.set_mode(0o700);
        std::fs::set_permissions(dir, perms).map_err(TransportError::Io)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        if !dir.exists() {
            std::fs::create_dir_all(dir).map_err(TransportError::Io)?;
        }
        Ok(())
    }
}

/// Resolves the socket path according to configuration or defaults.
///
/// If custom_path is provided: canonicalizes and validates it.
/// If not provided:
///   1. Check `$XDG_RUNTIME_DIR`. If set, use `$XDG_RUNTIME_DIR/agent-studios/agsd.sock`.
///   2. Fallback to `/tmp/agent-studios-<UID>/agsd.sock`.
///
/// Ensures the enclosing directory exists with mode `0700` and validates permissions.
pub fn resolve_socket_path(custom_path: Option<&Path>) -> Result<PathBuf, TransportError> {
    if let Some(path) = custom_path {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let parent = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };

        if !parent.exists() {
            create_secure_dir(parent)?;
        }
        validate_directory_permissions(parent)?;

        let canonical_parent = std::fs::canonicalize(parent).map_err(TransportError::Io)?;
        validate_directory_permissions(&canonical_parent)?;

        let file_name = path.file_name().ok_or_else(|| {
            TransportError::PermissionDenied("Invalid custom path: missing file name".to_string())
        })?;

        let canonical_socket_path = canonical_parent.join(file_name);
        Ok(canonical_socket_path)
    } else {
        let dir = if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
            if !xdg.trim().is_empty() {
                PathBuf::from(xdg).join("agent-studios")
            } else {
                let uid = get_current_uid();
                PathBuf::from(format!("/tmp/agent-studios-{}", uid))
            }
        } else {
            let uid = get_current_uid();
            PathBuf::from(format!("/tmp/agent-studios-{}", uid))
        };

        create_secure_dir(&dir)?;
        let canonical_dir = std::fs::canonicalize(&dir).unwrap_or(dir);
        validate_directory_permissions(&canonical_dir)?;

        let socket_path = canonical_dir.join("agsd.sock");
        Ok(socket_path)
    }
}

/// Sets mode 0600 on the socket file and validates ownership and permissions.
/// Fails closed (TransportError::PermissionDenied) if world/group accessible or wrong owner.
pub fn ensure_socket_permissions(path: &Path) -> Result<(), TransportError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let metadata = std::fs::symlink_metadata(path).map_err(TransportError::Io)?;
        let mut perms = metadata.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms).map_err(TransportError::Io)?;

        let updated = std::fs::symlink_metadata(path).map_err(TransportError::Io)?;
        let mode = updated.mode();
        let owner_uid = updated.uid();
        let current_uid = get_current_uid();

        validate_socket_mode_and_owner(mode, owner_uid, current_uid)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// If socket file exists, attempts a non-blocking connect.
/// If connection refused (stale socket), removes the socket file.
/// If active, fails closed with TransportError::PermissionDenied.
pub fn cleanup_stale_socket(path: &Path) -> Result<(), TransportError> {
    #[cfg(unix)]
    {
        if std::fs::symlink_metadata(path).is_ok() {
            match std::os::unix::net::UnixStream::connect(path) {
                Ok(stream) => {
                    drop(stream);
                    return Err(TransportError::PermissionDenied(format!(
                        "Active daemon socket already listening at {:?}",
                        path
                    )));
                }
                Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                    std::fs::remove_file(path).map_err(TransportError::Io)?;
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    // Ignored
                }
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::WouldBlock {
                        return Err(TransportError::PermissionDenied(format!(
                            "Socket at {:?} is actively in use (connection would block)",
                            path
                        )));
                    }
                    return Err(TransportError::Io(err));
                }
            }
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_validate_dir_mode_and_owner_success() {
        assert!(validate_dir_mode_and_owner(0o700, 1000, 1000).is_ok());
        // Extra file type bits (e.g. S_IFDIR = 0o040000)
        assert!(validate_dir_mode_and_owner(0o040700, 1000, 1000).is_ok());
    }

    #[test]
    fn test_validate_dir_mode_and_owner_rejection() {
        // Wrong owner
        assert!(matches!(
            validate_dir_mode_and_owner(0o700, 1001, 1000),
            Err(TransportError::PermissionDenied(_))
        ));

        // Group readable/executable (0750)
        assert!(matches!(
            validate_dir_mode_and_owner(0o750, 1000, 1000),
            Err(TransportError::PermissionDenied(_))
        ));

        // World readable/executable (0755)
        assert!(matches!(
            validate_dir_mode_and_owner(0o755, 1000, 1000),
            Err(TransportError::PermissionDenied(_))
        ));

        // World writable (0777)
        assert!(matches!(
            validate_dir_mode_and_owner(0o777, 1000, 1000),
            Err(TransportError::PermissionDenied(_))
        ));

        // Missing owner execute (0600)
        assert!(matches!(
            validate_dir_mode_and_owner(0o600, 1000, 1000),
            Err(TransportError::PermissionDenied(_))
        ));
    }

    #[test]
    fn test_validate_socket_mode_and_owner_success() {
        assert!(validate_socket_mode_and_owner(0o600, 1000, 1000).is_ok());
        // Extra file type bits (e.g. S_IFSOCK = 0o140000)
        assert!(validate_socket_mode_and_owner(0o140600, 1000, 1000).is_ok());
    }

    #[test]
    fn test_validate_socket_mode_and_owner_rejection() {
        // Wrong owner
        assert!(matches!(
            validate_socket_mode_and_owner(0o600, 1001, 1000),
            Err(TransportError::PermissionDenied(_))
        ));

        // Group readable (0640)
        assert!(matches!(
            validate_socket_mode_and_owner(0o640, 1000, 1000),
            Err(TransportError::PermissionDenied(_))
        ));

        // World readable (0644)
        assert!(matches!(
            validate_socket_mode_and_owner(0o644, 1000, 1000),
            Err(TransportError::PermissionDenied(_))
        ));

        // World writable (0666)
        assert!(matches!(
            validate_socket_mode_and_owner(0o666, 1000, 1000),
            Err(TransportError::PermissionDenied(_))
        ));

        // Executable (0700)
        assert!(matches!(
            validate_socket_mode_and_owner(0o700, 1000, 1000),
            Err(TransportError::PermissionDenied(_))
        ));

        // Read-only (0400)
        assert!(matches!(
            validate_socket_mode_and_owner(0o400, 1000, 1000),
            Err(TransportError::PermissionDenied(_))
        ));
    }

    #[test]
    fn test_resolve_socket_path_xdg() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp_dir = tempfile::tempdir().unwrap();
        let original_xdg = std::env::var("XDG_RUNTIME_DIR").ok();

        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", temp_dir.path());
        }
        let result = resolve_socket_path(None);

        unsafe {
            if let Some(orig) = original_xdg {
                std::env::set_var("XDG_RUNTIME_DIR", orig);
            } else {
                std::env::remove_var("XDG_RUNTIME_DIR");
            }
        }

        let path = result.expect("resolve_socket_path failed");
        assert_eq!(path.file_name().unwrap(), "agsd.sock");
        assert!(path.parent().unwrap().ends_with("agent-studios"));
    }

    #[test]
    fn test_resolve_socket_path_custom() {
        let temp_dir = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(temp_dir.path()).unwrap().permissions();
            perms.set_mode(0o700);
            std::fs::set_permissions(temp_dir.path(), perms).unwrap();
        }
        let custom_file = temp_dir.path().join("my-custom.sock");

        let resolved = resolve_socket_path(Some(&custom_file)).expect("resolve custom path failed");
        assert_eq!(resolved.file_name().unwrap(), "my-custom.sock");
    }

    #[test]
    fn test_resolve_socket_path_fallback() {
        let _guard = ENV_LOCK.lock().unwrap();
        let original_xdg = std::env::var("XDG_RUNTIME_DIR").ok();

        unsafe {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        let result = resolve_socket_path(None);

        unsafe {
            if let Some(orig) = original_xdg {
                std::env::set_var("XDG_RUNTIME_DIR", orig);
            } else {
                std::env::remove_var("XDG_RUNTIME_DIR");
            }
        }

        let path = result.expect("resolve_socket_path fallback failed");
        assert_eq!(path.file_name().unwrap(), "agsd.sock");
        let uid = get_current_uid();
        let parent_str = path.parent().unwrap().to_string_lossy();
        assert!(
            parent_str.contains(&format!("agent-studios-{}", uid)),
            "Expected parent to contain agent-studios-{}, got {}",
            uid,
            parent_str
        );

        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }
}
