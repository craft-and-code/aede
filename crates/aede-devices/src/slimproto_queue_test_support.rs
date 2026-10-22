//! Captures actual controller frames and emits replies only after each start.

use super::*;

#[derive(Default)]
pub(super) struct QueueWriter {
    bytes: Vec<u8>,
    parsed: usize,
    frames: Vec<([u8; 4], Vec<u8>)>,
    progress: Option<mpsc::Sender<Result<Packet, String>>>,
}

impl QueueWriter {
    pub(super) fn with_progress(progress: mpsc::Sender<Result<Packet, String>>) -> Self {
        Self {
            progress: Some(progress),
            ..Self::default()
        }
    }

    pub(super) fn starts(&self) -> Vec<&[u8]> {
        self.frames
            .iter()
            .filter(|(opcode, payload)| opcode == b"strm" && payload.first() == Some(&b's'))
            .map(|(_, payload)| payload.as_slice())
            .collect()
    }
}

pub(super) fn successful_http() -> Packet {
    Packet {
        opcode: *b"RESP",
        payload: b"HTTP/1.1 200 OK\r\n\r\n".to_vec(),
    }
}

impl AsyncWrite for QueueWriter {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        buffer: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.bytes.extend_from_slice(buffer);
        loop {
            let remaining = &self.bytes[self.parsed..];
            if remaining.len() < 2 {
                break;
            }
            let length = usize::from(u16::from_be_bytes([remaining[0], remaining[1]]));
            assert!(length >= 4, "invalid controller frame length");
            if remaining.len() < length + 2 {
                break;
            }
            let opcode = [remaining[2], remaining[3], remaining[4], remaining[5]];
            let payload = remaining[6..length + 2].to_vec();
            let starts = opcode == *b"strm" && payload.first() == Some(&b's');
            self.parsed += length + 2;
            self.frames.push((opcode, payload));
            if starts && let Some(sender) = &self.progress {
                for packet in [
                    successful_http(),
                    stat(b"STMs", 100),
                    Packet {
                        opcode: *b"DSCO",
                        payload: vec![0],
                    },
                    stat(b"STMd", 100),
                    stat(b"STMu", 0),
                ] {
                    sender.try_send(Ok(packet)).unwrap();
                }
            }
        }
        std::task::Poll::Ready(Ok(buffer.len()))
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}
