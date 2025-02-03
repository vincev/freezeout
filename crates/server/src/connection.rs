// Copyright (C) 2025  Vince Vasta.
// SPDX-License-Identifier: Apache-2.0

//! Noise protocol encrypted WebSocket connection types.
use anyhow::{anyhow, bail, Result};
use futures_util::{SinkExt, StreamExt};
use snow::{params::NoiseParams, TransportState};
use std::sync::LazyLock;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    self as websocket,
    tungstenite::{protocol::WebSocketConfig, Message as WsMessage},
    MaybeTlsStream, WebSocketStream,
};

use freezeout_core::message::SignedMessage;

static NOISE_PARAMS: LazyLock<NoiseParams> =
    LazyLock::new(|| "Noise_NN_25519_ChaChaPoly_BLAKE2s".parse().unwrap());

/// Maximum message length.
const MAX_MSG_LEN: usize = 16384;

/// A noise protocol encrypted WebSocket connection for [SignedMessage].
pub struct EncryptedConnection {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    transport: TransportState,
}

impl EncryptedConnection {
    /// Creates a new connection.
    fn new(stream: WebSocketStream<MaybeTlsStream<TcpStream>>, transport: TransportState) -> Self {
        Self { stream, transport }
    }

    /// Sends a [SignedMessage].
    pub async fn send(&mut self, msg: &SignedMessage) -> Result<()> {
        let mut buf = [0u8; MAX_MSG_LEN];
        let len = self.transport.write_message(&msg.serialize(), &mut buf)?;
        self.stream.send(WsMessage::binary(&buf[..len])).await?;

        Ok(())
    }

    /// Waits for a [SignedMessage].
    pub async fn recv(&mut self) -> Option<Result<SignedMessage>> {
        let mut buf = [0u8; MAX_MSG_LEN];
        loop {
            match self.stream.next().await {
                Some(Ok(WsMessage::Binary(payload))) => {
                    break Some(
                        self.transport
                            .read_message(payload.as_slice(), &mut buf)
                            .map_err(anyhow::Error::from)
                            .and_then(|len| SignedMessage::deserialize_and_verify(&buf[..len])),
                    );
                }
                Some(Ok(_)) => continue,
                Some(Err(e)) => break Some(Err(anyhow!("Connection error: {e}"))),
                None => break None,
            }
        }
    }

    /// Closes this connection.
    pub async fn close(&mut self) {
        let _ = self.stream.close(None).await;
    }
}

/// Creates an [EncryptedConnection] from a server stream.
pub async fn accept_async(stream: TcpStream) -> Result<EncryptedConnection> {
    let config = WebSocketConfig::default().max_message_size(Some(MAX_MSG_LEN));

    let mut stream =
        websocket::accept_async_with_config(MaybeTlsStream::Plain(stream), Some(config)).await?;

    // Start Noise protocol handshake with the client.
    let mut noise = snow::Builder::new(NOISE_PARAMS.clone()).build_responder()?;
    let mut buf = [0u8; MAX_MSG_LEN];

    // <- e
    match stream.next().await {
        Some(Ok(WsMessage::Binary(payload))) => {
            noise
                .read_message(payload.as_slice(), &mut buf)
                .map_err(|e| anyhow!("Responder Noise handshake invalid message {e}"))?;
        }
        Some(Ok(_)) => {
            bail!("Responder Noise handshake failed non binary stream");
        }
        Some(Err(e)) => bail!("Responder Noise handshake failed {e}"),
        None => bail!("Responder Noise handshake failed stream closed"),
    };

    // -> e, ee
    let len = noise.write_message(&[], &mut buf)?;
    stream.send(WsMessage::binary(&buf[..len])).await?;

    let transport = noise.into_transport_mode()?;

    Ok(EncryptedConnection::new(stream, transport))
}

/// Connects to a server and returns an [EncryptedConnection] if successful.
pub async fn connect_async(addr: &str) -> Result<EncryptedConnection> {
    let config = WebSocketConfig::default().max_message_size(Some(MAX_MSG_LEN));

    // Connect to server.
    let url = format!("ws://{}", addr);
    let (mut stream, _) = websocket::connect_async_with_config(&url, Some(config), false).await?;

    // Start Noise protocol handshake.
    let mut noise = snow::Builder::new(NOISE_PARAMS.clone()).build_initiator()?;
    let mut buf = [0u8; MAX_MSG_LEN];

    // -> e
    let len = noise.write_message(&[], &mut buf)?;
    stream.send(WsMessage::binary(&buf[..len])).await?;

    // <- e, ee
    match stream.next().await {
        Some(Ok(WsMessage::Binary(payload))) => {
            noise
                .read_message(payload.as_slice(), &mut buf)
                .map_err(|e| anyhow!("Initiator Noise handshake invalid message {e}"))?;
        }
        Some(Ok(_)) => {
            bail!("Initiator Noise handshake failed non binary stream");
        }
        Some(Err(e)) => bail!("Initiator Noise handshake failed {e}"),
        None => bail!("Initiator Noise handshake failed stream closed"),
    };

    let transport = noise.into_transport_mode()?;

    Ok(EncryptedConnection::new(stream, transport))
}

#[cfg(test)]
mod tests {
    use super::*;
    use freezeout_core::{crypto::SigningKey, message::Message};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn encrypted_websocket_connection() {
        let addr = "127.0.0.1:12345";

        let (tx, rx) = tokio::sync::oneshot::channel();

        let listener = TcpListener::bind(addr).await.unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut con = accept_async(stream).await.unwrap();

            let msg = con.recv().await.unwrap().unwrap();
            assert!(matches!(msg.message(), Message::JoinServer { nickname} if nickname == "Bob"));

            let msg = con.recv().await.unwrap().unwrap();
            assert!(matches!(msg.message(), Message::Error(e) if e == "error"));

            tx.send(()).unwrap();
        });

        let mut con = connect_async(addr).await.unwrap();
        let keypair = SigningKey::default();
        let msg = SignedMessage::new(
            &keypair,
            Message::JoinServer {
                nickname: "Bob".to_string(),
            },
        );
        con.send(&msg).await.unwrap();

        let msg = SignedMessage::new(&keypair, Message::Error("error".to_string()));
        con.send(&msg).await.unwrap();

        rx.await.unwrap();
    }
}
