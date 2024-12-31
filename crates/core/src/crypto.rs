// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Cryptographic types for signing messages.
use anyhow::Result;
use bip32::Mnemonic;
use blake2::{digest, digest::typenum::ToInt, Blake2s, Digest};
use ed25519_dalek::{Signer, Verifier};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A key for signing messages.
pub struct SigningKey(ed25519_dalek::SigningKey);

/// The hasher used for signatures.
type SigHasher = Blake2s<digest::consts::U32>;

impl Default for SigningKey {
    fn default() -> Self {
        let mut rng = rand::thread_rng();
        Self(ed25519_dalek::SigningKey::generate(&mut rng))
    }
}

impl SigningKey {
    /// Create a signing key from a mnemonic phrase.
    pub fn from_phrase<S>(phrase: S) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let mnemonic = Mnemonic::new(phrase, Default::default())?;
        Ok(Self(ed25519_dalek::SigningKey::from_bytes(
            mnemonic.entropy(),
        )))
    }

    /// Sign a message.
    pub fn sign<T>(&self, msg: &T) -> Signature
    where
        T: Serialize,
    {
        let mut hasher = SigHasher::new();
        bincode::serialize_into(&mut hasher, msg).expect("should serialize to hasher");
        Signature(self.0.sign(&hasher.finalize()))
    }

    /// Get the secret key phrase.
    pub fn phrase(&self) -> String {
        let mnemonic = Mnemonic::from_entropy(*self.0.as_bytes(), Default::default());
        mnemonic.phrase().to_string()
    }

    /// Get the signature verifying key.
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey(self.0.verifying_key())
    }
}

impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SigningKey({})",
            bs58::encode(self.0.as_bytes()).into_string()
        )
    }
}

/// Message signature.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Signature(ed25519_dalek::Signature);

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Signature({})",
            bs58::encode(&self.0.to_bytes()).into_string()
        )
    }
}

/// Key for signature verification.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct VerifyingKey(ed25519_dalek::VerifyingKey);

impl VerifyingKey {
    /// Verifies a message signature.
    pub fn verify<T>(&self, msg: &T, signature: &Signature) -> bool
    where
        T: Serialize,
    {
        let mut hasher = SigHasher::new();
        bincode::serialize_into(&mut hasher, msg).expect("should serialize to hasher");
        self.0.verify(&hasher.finalize(), &signature.0).is_ok()
    }

    /// Returns the [PeerId] for this key.
    pub fn peer_id(&self) -> PeerId {
        let mut hasher = Blake2s::<digest::consts::U16>::new();
        hasher.update(self.0.as_bytes());
        PeerId(hasher.finalize().into())
    }
}

impl fmt::Debug for VerifyingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "VerifyingKey({})",
            bs58::encode(self.0.as_bytes()).into_string()
        )
    }
}

/// A message sender identifier derived from a signature verifying key.
#[derive(Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct PeerId([u8; digest::consts::U16::INT]);

impl PeerId {
    /// The hex digits for this peer id.
    pub fn digits(&self) -> String {
        self.0
            .iter()
            .fold(String::with_capacity(32), |mut output, b| {
                output.push_str(&format!("{b:02X}"));
                output
            })
    }
}

impl fmt::Debug for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PeerId({})", self.digits())
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.digits())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keypair_phrase() {
        let sk = SigningKey::default();
        let from_phrase = SigningKey::from_phrase(sk.phrase()).unwrap();
        assert_eq!(sk.0, from_phrase.0);
    }

    #[test]
    fn sign() {
        #[derive(Serialize)]
        struct Point {
            x: f32,
            y: f32,
        }

        let msg = Point { x: 10.2, y: 4.3 };

        let sk = SigningKey::default();
        let sig = sk.sign(&msg);

        // Signed message
        let vk = sk.verifying_key();
        assert!(vk.verify(&msg, &sig));

        // Invalid message
        let msg = Point { x: 10.2001, y: 4.3 };
        assert!(!vk.verify(&msg, &sig));
    }
}
