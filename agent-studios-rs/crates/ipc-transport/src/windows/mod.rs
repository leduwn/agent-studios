pub mod named_pipe;
pub mod security;

pub use named_pipe::*;
pub use security::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::{DEFAULT_MAX_FRAME_SIZE, recv_frame, send_frame};

    #[test]
    fn test_security_descriptor_creation_succeeds() {
        let sa = UserOnlySecurityAttributes::new().expect("Failed to create security descriptor");
        assert!(!sa.as_security_attributes_ptr().is_null());
        assert!(!sa.as_security_descriptor_ptr().is_null());
        assert!(sa.is_valid());
    }

    #[tokio::test]
    async fn test_named_pipe_roundtrip() {
        let pipe_id = uuid::Uuid::new_v4().to_string();
        let pipe_name = default_pipe_name(&pipe_id);

        let mut listener = NamedPipeListener::bind(&pipe_name).expect("Failed to bind listener");

        let pipe_name_clone = pipe_name.clone();
        let server_task = tokio::spawn(async move {
            let mut server = listener.accept().await.expect("Accept failed");
            let payload = recv_frame(&mut server, DEFAULT_MAX_FRAME_SIZE)
                .await
                .expect("Server recv failed");
            send_frame(&mut server, &payload)
                .await
                .expect("Server send echo failed");
        });

        let client_task = tokio::spawn(async move {
            let mut stream = NamedPipeStream::connect(&pipe_name_clone)
                .await
                .expect("Connect failed");
            let message = b"agent-studios-ipc-named-pipe-roundtrip-verified";
            send_frame(&mut stream, message)
                .await
                .expect("Client send failed");
            let received = recv_frame(&mut stream, DEFAULT_MAX_FRAME_SIZE)
                .await
                .expect("Client recv failed");
            assert_eq!(received, message);
        });

        let (srv_res, cli_res) = tokio::join!(server_task, client_task);
        srv_res.expect("Server task panicked");
        cli_res.expect("Client task panicked");
    }

    #[tokio::test]
    async fn test_invalid_pipe_name_rejected() {
        let invalid_names = [
            "",
            "agent-studios-pipe",
            r"\pipe\agent-studios",
            r"\\pipe\agent-studios",
            r"//./pipe/agent-studios",
            r"\\.\pipe\",
        ];

        for name in invalid_names {
            let bind_res = NamedPipeListener::bind(name);
            assert!(bind_res.is_err(), "Expected bind to fail for {:?}", name);

            let connect_res = NamedPipeStream::connect(name).await;
            assert!(
                connect_res.is_err(),
                "Expected connect to fail for {:?}",
                name
            );
        }
    }
}
