// Copyright (C) 2025  Vince Vasta.
// SPDX-License-Identifier: Apache-2.0

//! TLS and Noise protocol encrypted WebSocket connection types.
use anyhow::{Result, anyhow, bail};
use bytes::BytesMut;
use futures_util::{SinkExt, StreamExt};
use snow::{TransportState, params::NoiseParams};
use std::sync::LazyLock;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_tungstenite::{
    self as websocket, MaybeTlsStream, WebSocketStream,
    tungstenite::{Message as WsMessage, protocol::WebSocketConfig},
};

use crate::message::SignedMessage;

static NOISE_PARAMS: LazyLock<NoiseParams> =
    LazyLock::new(|| "Noise_NN_25519_ChaChaPoly_BLAKE2s".parse().unwrap());

/// Maximum message length.
const MAX_MSG_LEN: usize = 16384;

/// The client connection type.
pub type ClientConnection = EncryptedConnection<MaybeTlsStream<TcpStream>>;

/// A noise protocol encrypted WebSocket connection for [SignedMessage].
pub struct EncryptedConnection<S> {
    stream: WebSocketStream<S>,
    transport: TransportState,
}

impl<S> EncryptedConnection<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    /// Sends a [SignedMessage].
    pub async fn send(&mut self, msg: &SignedMessage) -> Result<()> {
        let mut buf = BytesMut::zeroed(MAX_MSG_LEN);
        let len = self.transport.write_message(&msg.serialize(), &mut buf)?;
        self.stream
            .send(WsMessage::binary(buf.freeze().slice(..len)))
            .await?;
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
                            .read_message(&payload, &mut buf)
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
pub async fn accept_async<S>(stream: S) -> Result<EncryptedConnection<S>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let config = WebSocketConfig::default().max_message_size(Some(MAX_MSG_LEN));
    let mut stream = websocket::accept_async_with_config(stream, Some(config)).await?;

    // Start Noise protocol handshake with the client.
    let mut noise = snow::Builder::new(NOISE_PARAMS.clone()).build_responder()?;
    let mut buf = BytesMut::zeroed(MAX_MSG_LEN);

    // <- e
    match stream.next().await {
        Some(Ok(WsMessage::Binary(payload))) => {
            noise
                .read_message(&payload, &mut buf)
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
    stream
        .send(WsMessage::binary(buf.freeze().slice(..len)))
        .await?;

    let transport = noise.into_transport_mode()?;

    Ok(EncryptedConnection { stream, transport })
}

/// Connects to a server and returns an [EncryptedConnection] if successful.
pub async fn connect_async(url: &str) -> Result<ClientConnection> {
    let config = WebSocketConfig::default().max_message_size(Some(MAX_MSG_LEN));
    let (mut stream, _) = websocket::connect_async_with_config(url, Some(config), false).await?;

    // Start Noise protocol handshake.
    let mut noise = snow::Builder::new(NOISE_PARAMS.clone()).build_initiator()?;

    // -> e
    let mut buf = BytesMut::zeroed(MAX_MSG_LEN);
    let len = noise.write_message(&[], &mut buf)?;
    stream
        .send(WsMessage::binary(buf.freeze().slice(..len)))
        .await?;

    // <- e, ee
    match stream.next().await {
        Some(Ok(WsMessage::Binary(payload))) => {
            let mut buf = BytesMut::zeroed(MAX_MSG_LEN);
            noise
                .read_message(&payload, &mut buf)
                .map_err(|e| anyhow!("Initiator Noise handshake invalid message {e}"))?;
        }
        Some(Ok(_)) => {
            bail!("Initiator Noise handshake failed non binary stream");
        }
        Some(Err(e)) => bail!("Initiator Noise handshake failed {e}"),
        None => bail!("Initiator Noise handshake failed stream closed"),
    };

    let transport = noise.into_transport_mode()?;
    Ok(EncryptedConnection { stream, transport })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{crypto::SigningKey, message::Message};
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
            assert!(matches!(msg.message(), Message::JoinTable));

            tx.send(()).unwrap();
        });

        let url = format!("ws://{addr}");
        let mut con = connect_async(&url).await.unwrap();
        let keypair = SigningKey::default();
        let msg = SignedMessage::new(
            &keypair,
            Message::JoinServer {
                nickname: "Bob".to_string(),
            },
        );
        con.send(&msg).await.unwrap();

        let msg = SignedMessage::new(&keypair, Message::JoinTable);
        con.send(&msg).await.unwrap();

        rx.await.unwrap();
    }
}
