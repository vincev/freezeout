// Copyright (C) 2025 Vince Vasta
// SPDX-License-Identifier: Apache-2.0

//! Cryptographic types for signing messages.
use anyhow::{Result, bail};
use bip39::Mnemonic;
use blake2::{Blake2s, Digest, digest, digest::typenum::ToInt};
use ed25519_dalek::{Signer, Verifier};
use rand::{CryptoRng, RngCore, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::Zeroizing;

const ENTROPY_LEN: usize = 16;
type Entropy = [u8; ENTROPY_LEN];

/// A key for signing messages.
pub struct SigningKey {
    key: ed25519_dalek::SigningKey,
    entropy: Zeroizing<Entropy>,
}

/// The hasher used for signatures.
type SigHasher = Blake2s<digest::consts::U32>;

impl Default for SigningKey {
    fn default() -> Self {
        let mut rng = StdRng::from_os_rng();
        Self::from_crypto_rng(&mut rng)
    }
}

impl SigningKey {
    /// Create a signing key from a mnemonic phrase.
    pub fn from_phrase<S>(phrase: S) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let mnemonic = Mnemonic::from_phrase(phrase.as_ref(), Default::default())?;
        if mnemonic.entropy().len() != ENTROPY_LEN {
            bail!("Invalid passphrase length");
        }

        let mut entropy = Entropy::default();
        entropy.copy_from_slice(mnemonic.entropy());
        Ok(Self::from_entropy(entropy))
    }

    /// Sign a message.
    pub fn sign<T>(&self, msg: &T) -> Signature
    where
        T: Serialize,
    {
        let mut hasher = SigHasher::new();
        bincode::serialize_into(&mut hasher, msg).expect("should serialize to hasher");
        Signature(self.key.sign(&hasher.finalize()))
    }

    /// Get the secret key phrase.
    pub fn phrase(&self) -> String {
        // This should never fail as we control the entropy size.
        Mnemonic::from_entropy(self.entropy.as_ref(), Default::default())
            .unwrap()
            .phrase()
            .to_string()
    }

    /// Get the signature verifying key.
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey(self.key.verifying_key())
    }

    fn from_crypto_rng<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        let mut entropy = Entropy::default();
        rng.fill_bytes(&mut entropy);
        Self::from_entropy(entropy)
    }

    fn from_entropy(entropy: Entropy) -> Self {
        // Hash 128 bits entropy to 256 bits SigningKey.
        let key_hash = SigHasher::digest(entropy);
        let key = ed25519_dalek::SigningKey::from_bytes(&key_hash.into());
        let entropy = Zeroizing::new(entropy);
        Self { key, entropy }
    }
}

impl fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SigningKey({})",
            bs58::encode(self.key.as_bytes()).into_string()
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
        assert_eq!(sk.key, from_phrase.key);
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
