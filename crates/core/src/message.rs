// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Type definitions for messages between the client and server.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::crypto::{PlayerId, Signature, SigningKey, VerifyingKey};

/// Message exchanged by a client and a server.
#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    /// Join a table with a nickname.
    JoinTable(String),
    /// An error message.
    Error(String),
}

/// A signed message.
#[derive(Debug, Serialize, Deserialize)]
pub struct SignedMessage {
    msg: Message,
    sig: Signature,
    vk: VerifyingKey,
}

impl SignedMessage {
    /// Creates a new signed message.
    pub fn new(sk: &SigningKey, msg: Message) -> Self {
        let sig = sk.sign(&msg);
        Self {
            msg,
            sig,
            vk: sk.verifying_key(),
        }
    }

    /// Deserializes this message and verifies its signature.
    pub fn deserialize_and_verify(buf: &[u8]) -> Result<Self> {
        let sm = bincode::deserialize::<Self>(buf)?;
        if !sm.vk.verify(&sm.msg, &sm.sig) {
            bail!("Invalid signature");
        }

        Ok(sm)
    }

    /// Serializes this message.
    pub fn serialize(&self) -> Vec<u8> {
        bincode::serialize(self).expect("Should serialize signed message")
    }

    /// Returns the identifier of the player who sent this message.
    pub fn player_id(&self) -> PlayerId {
        self.vk.player_id()
    }

    /// Extracts the signed message.
    pub fn to_message(self) -> Message {
        self.msg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_message() {
        let keypair = SigningKey::default();
        let message = Message::JoinTable("Alice".to_string());

        let smsg = SignedMessage::new(&keypair, message);
        let bytes = smsg.serialize();

        let deser_msg = SignedMessage::deserialize_and_verify(&bytes)
            .map(|sm| sm.to_message())
            .unwrap();
        assert!(matches!(deser_msg, Message::JoinTable(s) if s == "Alice"));
    }
}
