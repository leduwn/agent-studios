use crate::error::TransportError;
use crate::windows::security::UserOnlySecurityAttributes;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;

pub const PIPE_PREFIX: &str = r"\\.\pipe\";

/// Generates a default named pipe path for a given identifier.
pub fn default_pipe_name(id: &str) -> String {
    format!(r"\\.\pipe\agent-studios-{}", id)
}

/// Generates a named pipe path for a given identifier.
pub fn pipe_name_for(id: &str) -> String {
    default_pipe_name(id)
}

/// Validates that a named pipe path starts with the mandatory `\\.\pipe\` prefix
/// and has a non-empty pipe name.
pub fn validate_pipe_name(name: &str) -> Result<(), TransportError> {
    if !name.starts_with(PIPE_PREFIX) || name.len() <= PIPE_PREFIX.len() {
        return Err(TransportError::SecurityDescriptor(format!(
            "Invalid named pipe path '{}': must start with '{}' and contain a pipe name",
            name, PIPE_PREFIX
        )));
    }
    Ok(())
}

/// Named pipe server listener that creates instances protected by
/// user-only security attributes.
pub struct NamedPipeListener {
    pipe_name: String,
    server: Option<NamedPipeServer>,
    security_attributes: UserOnlySecurityAttributes,
}

impl NamedPipeListener {
    /// Binds a new named pipe listener at the given path.
    /// Creates the initial pipe instance with user-only security attributes.
    pub fn bind(pipe_name: impl Into<String>) -> Result<Self, TransportError> {
        let pipe_name = pipe_name.into();
        validate_pipe_name(&pipe_name)?;

        let security_attributes = UserOnlySecurityAttributes::new()?;
        let server = unsafe {
            ServerOptions::new()
                .first_pipe_instance(true)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    &pipe_name,
                    security_attributes.as_security_attributes_mut_ptr(),
                )?
        };

        Ok(Self {
            pipe_name,
            server: Some(server),
            security_attributes,
        })
    }

    /// Returns the pipe name this listener is bound to.
    pub fn pipe_name(&self) -> &str {
        &self.pipe_name
    }

    /// Accepts an incoming connection on the named pipe.
    /// After accepting, immediately creates the next pipe instance
    /// so subsequent connections can be serviced.
    pub async fn accept(&mut self) -> Result<NamedPipeServer, TransportError> {
        let server = self
            .server
            .take()
            .ok_or_else(|| TransportError::InvalidState {
                expected: "active named pipe server instance".into(),
                actual: "none".into(),
            })?;

        server.connect().await?;

        // Instantiate next server endpoint with identical user-only security attributes
        let next_server = unsafe {
            ServerOptions::new()
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(
                    &self.pipe_name,
                    self.security_attributes.as_security_attributes_mut_ptr(),
                )?
        };
        self.server = Some(next_server);

        Ok(server)
    }
}

/// Asynchronous client stream connecting to a named pipe.
pub struct NamedPipeStream {
    inner: NamedPipeClient,
}

impl NamedPipeStream {
    /// Connects to the named pipe at `pipe_name`.
    /// If the pipe is busy (`ERROR_PIPE_BUSY`), retries with exponential backoff for up to 3 seconds.
    pub async fn connect(pipe_name: impl AsRef<str>) -> Result<Self, TransportError> {
        let pipe_name = pipe_name.as_ref();
        validate_pipe_name(pipe_name)?;

        let start = Instant::now();
        let timeout = Duration::from_secs(3);
        let mut backoff = Duration::from_millis(10);
        let max_backoff = Duration::from_millis(250);

        loop {
            match ClientOptions::new().open(pipe_name) {
                Ok(client) => return Ok(NamedPipeStream { inner: client }),
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    if start.elapsed() >= timeout {
                        return Err(TransportError::Timeout);
                    }
                    tokio::time::sleep(backoff).await;
                    backoff = std::cmp::min(backoff.saturating_mul(2), max_backoff);
                }
                Err(e) => return Err(TransportError::Io(e)),
            }
        }
    }

    /// Consumes the stream and returns the underlying `NamedPipeClient`.
    pub fn into_inner(self) -> NamedPipeClient {
        self.inner
    }

    /// Returns a shared reference to the underlying `NamedPipeClient`.
    pub fn inner(&self) -> &NamedPipeClient {
        &self.inner
    }

    /// Returns a mutable reference to the underlying `NamedPipeClient`.
    pub fn inner_mut(&mut self) -> &mut NamedPipeClient {
        &mut self.inner
    }
}

impl Deref for NamedPipeStream {
    type Target = NamedPipeClient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for NamedPipeStream {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl AsyncRead for NamedPipeStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for NamedPipeStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
