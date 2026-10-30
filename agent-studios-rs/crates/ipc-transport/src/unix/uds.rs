use crate::error::TransportError;
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
use super::permissions::{
    cleanup_stale_socket, ensure_socket_permissions, get_current_uid, resolve_socket_path,
    validate_directory_permissions, validate_socket_mode_and_owner,
};

#[cfg(unix)]
pub struct UnixSocketListener {
    listener: tokio::net::UnixListener,
    socket_path: PathBuf,
}

#[cfg(unix)]
impl UnixSocketListener {
    /// Binds a Unix domain socket listener to the specified path.
    /// Validates directory permissions, cleans stale socket if needed, binds, and enforces 0600 permissions.
    pub fn bind(path: impl AsRef<Path>) -> Result<Self, TransportError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && parent.exists()
        {
            validate_directory_permissions(parent)?;
        }

        cleanup_stale_socket(path)?;

        let listener = tokio::net::UnixListener::bind(path).map_err(TransportError::Io)?;
        ensure_socket_permissions(path)?;

        Ok(Self {
            listener,
            socket_path: path.to_path_buf(),
        })
    }

    /// Binds using the default resolved socket path.
    pub fn bind_default() -> Result<Self, TransportError> {
        let path = resolve_socket_path(None)?;
        Self::bind(path)
    }

    /// Accepts an incoming connection returning the raw Tokio UnixStream.
    pub async fn accept(&self) -> Result<tokio::net::UnixStream, TransportError> {
        let (stream, _addr) = self.listener.accept().await.map_err(TransportError::Io)?;
        Ok(stream)
    }

    /// Accepts an incoming connection returning a wrapped UnixSocketStream.
    pub async fn accept_stream(&self) -> Result<UnixSocketStream, TransportError> {
        let stream = self.accept().await?;
        Ok(UnixSocketStream::new(stream))
    }

    /// Returns the socket path bound by this listener.
    pub fn path(&self) -> &Path {
        &self.socket_path
    }
}

#[cfg(unix)]
impl Drop for UnixSocketListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[cfg(unix)]
pub struct UnixSocketStream {
    inner: tokio::net::UnixStream,
}

#[cfg(unix)]
impl UnixSocketStream {
    /// Connects to a Unix domain socket at `path`.
    /// Validates socket permissions before and after connecting to prevent TOCTOU attacks.
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self, TransportError> {
        let path = path.as_ref();

        // 1. Validate permissions before connecting
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(path).map_err(TransportError::Io)?;
        let mode = metadata.mode();
        let owner_uid = metadata.uid();
        let current_uid = get_current_uid();
        validate_socket_mode_and_owner(mode, owner_uid, current_uid)?;

        // 2. Connect
        let stream = tokio::net::UnixStream::connect(path)
            .await
            .map_err(TransportError::Io)?;

        // 3. Re-validate socket permissions after connecting to detect replacement races
        let post_metadata = std::fs::symlink_metadata(path).map_err(TransportError::Io)?;
        validate_socket_mode_and_owner(post_metadata.mode(), post_metadata.uid(), current_uid)?;

        // 4. On Linux, check peer credentials if supported
        #[cfg(target_os = "linux")]
        {
            if let Ok(ucred) = stream.peer_cred()
                && ucred.uid() != current_uid
            {
                return Err(TransportError::PermissionDenied(format!(
                    "Peer UID {} does not match current UID {}",
                    ucred.uid(),
                    current_uid
                )));
            }
        }

        Ok(Self { inner: stream })
    }

    pub fn new(inner: tokio::net::UnixStream) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> tokio::net::UnixStream {
        self.inner
    }

    pub fn get_ref(&self) -> &tokio::net::UnixStream {
        &self.inner
    }

    pub fn get_mut(&mut self) -> &mut tokio::net::UnixStream {
        &mut self.inner
    }
}

#[cfg(unix)]
impl tokio::io::AsyncRead for UnixSocketStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

#[cfg(unix)]
impl tokio::io::AsyncWrite for UnixSocketStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

// ---------------------------------------------------------------------------
// Non-Unix Compatibility Mock Implementation
// ---------------------------------------------------------------------------

#[cfg(not(unix))]
#[derive(Debug)]
pub struct UnixSocketListener {
    _private: (),
}

#[cfg(not(unix))]
#[derive(Debug)]
pub struct UnixSocketStream {
    _private: (),
}

#[cfg(not(unix))]
impl UnixSocketListener {
    pub fn bind(_path: impl AsRef<Path>) -> Result<Self, TransportError> {
        Err(TransportError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix domain sockets not supported on this platform",
        )))
    }

    pub fn bind_default() -> Result<Self, TransportError> {
        Err(TransportError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix domain sockets not supported on this platform",
        )))
    }

    pub async fn accept(&self) -> Result<UnixSocketStream, TransportError> {
        Err(TransportError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix domain sockets not supported on this platform",
        )))
    }

    pub async fn accept_stream(&self) -> Result<UnixSocketStream, TransportError> {
        Err(TransportError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix domain sockets not supported on this platform",
        )))
    }

    pub fn path(&self) -> &Path {
        Path::new("")
    }
}

#[cfg(not(unix))]
impl UnixSocketStream {
    pub async fn connect(_path: impl AsRef<Path>) -> Result<Self, TransportError> {
        Err(TransportError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix domain sockets not supported on this platform",
        )))
    }
}

#[cfg(not(unix))]
impl tokio::io::AsyncRead for UnixSocketStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix domain sockets not supported on this platform",
        )))
    }
}

#[cfg(not(unix))]
impl tokio::io::AsyncWrite for UnixSocketStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix domain sockets not supported on this platform",
        )))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix domain sockets not supported on this platform",
        )))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Unix domain sockets not supported on this platform",
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(unix))]
    #[tokio::test]
    async fn test_non_unix_mock_uds() {
        let bind_res = UnixSocketListener::bind("dummy.sock");
        assert!(
            matches!(bind_res, Err(TransportError::Io(ref err)) if err.kind() == std::io::ErrorKind::Unsupported)
        );

        let bind_def = UnixSocketListener::bind_default();
        assert!(
            matches!(bind_def, Err(TransportError::Io(ref err)) if err.kind() == std::io::ErrorKind::Unsupported)
        );

        let dummy_listener = UnixSocketListener { _private: () };
        assert!(
            matches!(dummy_listener.accept().await, Err(TransportError::Io(ref err)) if err.kind() == std::io::ErrorKind::Unsupported)
        );
        assert!(
            matches!(dummy_listener.accept_stream().await, Err(TransportError::Io(ref err)) if err.kind() == std::io::ErrorKind::Unsupported)
        );

        let connect_res = UnixSocketStream::connect("dummy.sock").await;
        assert!(
            matches!(connect_res, Err(TransportError::Io(ref err)) if err.kind() == std::io::ErrorKind::Unsupported)
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_unix_socket_listener_lifecycle() {
        let temp_dir = tempfile::tempdir().unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(temp_dir.path()).unwrap().permissions();
            perms.set_mode(0o700);
            std::fs::set_permissions(temp_dir.path(), perms).unwrap();
        }
        let sock_path = temp_dir.path().join("test_lifecycle.sock");

        {
            let listener = UnixSocketListener::bind(&sock_path).expect("bind failed");
            assert!(sock_path.exists());
            assert_eq!(listener.path(), sock_path);
        }
        // Listener dropped: file should be removed by Drop
        assert!(!sock_path.exists());
    }
}
