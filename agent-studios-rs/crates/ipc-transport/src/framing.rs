use crate::error::TransportError;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const DEFAULT_MAX_FRAME_SIZE: usize = 4 * 1024 * 1024; // 4 MB

/// Reads a single length-delimited frame from an async reader.
/// Validates length prefix before allocating memory.
pub async fn recv_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
    max_size: usize,
) -> Result<Vec<u8>, TransportError> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(0) => return Err(TransportError::ConnectionClosed),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(TransportError::ConnectionClosed);
        }
        Err(e) => return Err(TransportError::Io(e)),
    }

    let length = u32::from_be_bytes(len_buf) as usize;
    if length > max_size {
        return Err(TransportError::FrameTooLarge {
            size: length,
            max: max_size,
        });
    }

    let mut payload = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut payload).await?;
    }
    Ok(payload)
}

/// Writes a single length-delimited frame to an async writer.
pub async fn send_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    payload: &[u8],
) -> Result<(), TransportError> {
    let length = payload.len() as u32;
    writer.write_all(&length.to_be_bytes()).await?;
    if !payload.is_empty() {
        writer.write_all(payload).await?;
    }
    writer.flush().await?;
    Ok(())
}

/// Helper to serialize and send a typed value.
pub async fn send_msg<W: AsyncWrite + Unpin, T: serde::Serialize>(
    writer: &mut W,
    value: &T,
) -> Result<(), TransportError> {
    let bytes = serde_json::to_vec(value)?;
    send_frame(writer, &bytes).await
}

/// Helper to receive and deserialize a typed value.
pub async fn recv_msg<R: AsyncRead + Unpin, T: serde::de::DeserializeOwned>(
    reader: &mut R,
    max_size: usize,
) -> Result<T, TransportError> {
    let bytes = recv_frame(reader, max_size).await?;
    let val = serde_json::from_slice(&bytes)?;
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_send_and_recv_frame() {
        let mut buffer = Vec::new();
        let payload = b"hello agent studios ipc";
        send_frame(&mut buffer, payload).await.unwrap();

        let mut cursor = Cursor::new(buffer);
        let received = recv_frame(&mut cursor, DEFAULT_MAX_FRAME_SIZE)
            .await
            .unwrap();
        assert_eq!(received, payload);
    }

    #[tokio::test]
    async fn test_oversized_frame_rejected_before_alloc() {
        let oversized = 10 * 1024 * 1024u32;
        let buffer = oversized.to_be_bytes().to_vec();
        let mut cursor = Cursor::new(buffer);

        let err = recv_frame(&mut cursor, 1024).await.unwrap_err();
        match err {
            TransportError::FrameTooLarge { size, max } => {
                assert_eq!(size, 10 * 1024 * 1024);
                assert_eq!(max, 1024);
            }
            other => panic!("Expected FrameTooLarge, got {:?}", other),
        }
    }
}
