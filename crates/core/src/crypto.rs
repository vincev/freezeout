// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Cryptographic types for signing messages.
use anyhow::Result;
use bip32::Mnemonic;
use blake2::{digest, digest::typenum::ToInt, Blake2b, Digest};
use ed25519_dalek::{Signer, Verifier};
use serde::{Deserialize, Serialize};
use std::fmt;

/// A key for signing messages.
pub struct SigningKey(ed25519_dalek::SigningKey);

/// Message signature.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Signature(ed25519_dalek::Signature);

/// Key for signature verification.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct VerifyingKey(ed25519_dalek::VerifyingKey);

/// Player identifier derived from a signature verifying key.
#[derive(Clone, Default, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct PlayerId(HashValue);

/// A hash value wrapper for serializable types.
#[derive(Clone, Default, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct HashValue([u8; digest::consts::U20::INT]);

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
        let hash = HashValue::from_serde(msg);
        Signature(self.0.sign(hash.as_bytes()))
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

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Signature({})",
            bs58::encode(&self.0.to_bytes()).into_string()
        )
    }
}

impl VerifyingKey {
    /// Verifies a message signature.
    pub fn verify<T>(&self, msg: &T, signature: &Signature) -> bool
    where
        T: Serialize,
    {
        let hash = HashValue::from_serde(msg);
        self.0.verify(hash.as_bytes(), &signature.0).is_ok()
    }

    /// Returns the [PlayerId] for this key.
    pub fn player_id(&self) -> PlayerId {
        PlayerId(HashValue::from_serde(self.0.as_bytes()))
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

impl fmt::Debug for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PlayerId({})",
            bs58::encode(&self.0.as_bytes()).into_string()
        )
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", bs58::encode(&self.0.as_bytes()).into_string())
    }
}

impl HashValue {
    /// Creates a [HashValue] from a serializable struct.
    pub fn from_serde<T>(t: &T) -> Self
    where
        T: Serialize,
    {
        let mut hasher = Blake2b::<digest::consts::U20>::new();
        bincode::serialize_into(&mut hasher, t).expect("should serialize to hasher");
        Self(hasher.finalize().into())
    }

    /// Returns a reference to this hash bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for HashValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HashValue({})", bs58::encode(&self.0).into_string())
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
