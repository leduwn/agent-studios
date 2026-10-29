pub mod error;
pub mod framing;

#[cfg(windows)]
pub mod windows;

pub mod unix;

pub mod client;
pub mod connection;
pub mod daemon;
pub mod server;

pub use client::{
    ClientConfig, EventSubscriptionStream, HealthResponse, IpcClient, ModelDefinition, PongResponse,
};
pub use connection::{
    ConnectionSession, ConnectionState, IpcCommand, IpcResponse, handle_connection,
};
pub use daemon::{DaemonConfig, run_daemon};
pub use error::TransportError;
pub use framing::{DEFAULT_MAX_FRAME_SIZE, recv_frame, recv_msg, send_frame, send_msg};
pub use server::{IpcServer, IpcServerBuilder, ServerState};
