//! Password-sealed blobs: Argon2id turns a master password into a key, and
//! ChaCha20-Poly1305 encrypts and authenticates the data.
//!
//! Layout: `QUAYVLT1` | memory KiB, passes, lanes (u32 LE each) | salt (16) |
//! nonce (12) | ciphertext and tag. Everything before the ciphertext is
//! authenticated, so changing the cost settings breaks decryption.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"QUAYVLT1";
const HEADER: usize = 8 + 12 + 16 + 12;

/// Argon2id cost. The defaults take roughly a second on a laptop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    pub memory_kib: u32,
    pub passes: u32,
    pub lanes: u32,
}

impl Default for Cost {
    fn default() -> Self {
        Self { memory_kib: 64 * 1024, passes: 3, lanes: 1 }
    }
}

/// The most a file may ask for, so a crafted file cannot make us allocate gigabytes.
const MAX: Cost = Cost { memory_kib: 512 * 1024, passes: 10, lanes: 16 };

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ArchiveError {
    #[error("this is not a quay-term vault file")]
    NotAVault,
    #[error("the vault file asks for more memory or time than quay-term allows")]
    TooCostly,
    #[error("wrong password, or the file is damaged")]
    WrongPasswordOrDamaged,
    #[error("could not encrypt: {0}")]
    Crypto(String),
}

fn derive(password: &str, salt: &[u8], cost: Cost) -> Result<Zeroizing<[u8; 32]>, ArchiveError> {
    let params = Params::new(cost.memory_kib, cost.passes, cost.lanes, Some(32))
        .map_err(|e| ArchiveError::Crypto(e.to_string()))?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|e| ArchiveError::Crypto(e.to_string()))?;
    Ok(key)
}

pub fn seal(password: &str, plaintext: &[u8]) -> Result<Vec<u8>, ArchiveError> {
    seal_with(password, plaintext, Cost::default())
}

pub fn seal_with(password: &str, plaintext: &[u8], cost: Cost) -> Result<Vec<u8>, ArchiveError> {
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut salt).map_err(|e| ArchiveError::Crypto(e.to_string()))?;
    getrandom::fill(&mut nonce).map_err(|e| ArchiveError::Crypto(e.to_string()))?;

    let mut out = Vec::with_capacity(HEADER + plaintext.len() + 16);
    out.extend_from_slice(MAGIC);
    for v in [cost.memory_kib, cost.passes, cost.lanes] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);

    let key = derive(password, &salt, cost)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()));
    let body = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad: &out })
        .map_err(|e| ArchiveError::Crypto(e.to_string()))?;
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn open(password: &str, data: &[u8]) -> Result<Zeroizing<Vec<u8>>, ArchiveError> {
    if data.len() < HEADER + 16 || &data[..8] != MAGIC {
        return Err(ArchiveError::NotAVault);
    }
    let u32_at = |i: usize| u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
    let cost = Cost { memory_kib: u32_at(8), passes: u32_at(12), lanes: u32_at(16) };
    if cost.memory_kib > MAX.memory_kib || cost.passes > MAX.passes || cost.lanes > MAX.lanes {
        return Err(ArchiveError::TooCostly);
    }
    let (salt, nonce) = (&data[20..36], &data[36..48]);
    let key = derive(password, salt, cost).map_err(|_| ArchiveError::NotAVault)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()));
    cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: &data[HEADER..], aad: &data[..HEADER] })
        .map(Zeroizing::new)
        .map_err(|_| ArchiveError::WrongPasswordOrDamaged)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: Cost = Cost { memory_kib: 64, passes: 1, lanes: 1 };

    #[test]
    fn round_trips() {
        let blob = seal_with("correct horse", b"secret data", FAST).unwrap();
        assert!(!blob.windows(11).any(|w| w == b"secret data"));
        assert_eq!(open("correct horse", &blob).unwrap().as_slice(), b"secret data");
    }

    #[test]
    fn wrong_password_and_tampering_are_refused() {
        let blob = seal_with("pw", b"data", FAST).unwrap();
        assert_eq!(open("other", &blob).unwrap_err(), ArchiveError::WrongPasswordOrDamaged);
        for i in [HEADER + 1, 9, 30, blob.len() - 1] {
            let mut bad = blob.clone();
            bad[i] ^= 1;
            assert!(open("pw", &bad).is_err(), "flipping byte {i} must fail");
        }
    }

    #[test]
    fn each_seal_uses_a_fresh_salt_and_nonce() {
        let a = seal_with("pw", b"x", FAST).unwrap();
        let b = seal_with("pw", b"x", FAST).unwrap();
        assert_ne!(a[20..48], b[20..48]);
    }

    #[test]
    fn rejects_non_vaults_and_hostile_cost() {
        assert_eq!(open("pw", b"hello").unwrap_err(), ArchiveError::NotAVault);
        assert_eq!(open("pw", &[0u8; 80]).unwrap_err(), ArchiveError::NotAVault);
        let mut blob = seal_with("pw", b"x", FAST).unwrap();
        blob[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(open("pw", &blob).unwrap_err(), ArchiveError::TooCostly);
    }
}
