//! Small values sealed with a raw 32-byte key (no password stretching), for data that is
//! written often, such as command history. The key itself lives inside the master-password
//! file (see [`crate::FileStore::history_key`]).
//!
//! Layout: nonce (12) | ciphertext and tag. `label` is authenticated, so a value sealed
//! for one column cannot be moved into another.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use zeroize::Zeroizing;

pub type FieldKey = Zeroizing<[u8; 32]>;

/// A fresh random key.
pub fn new_key() -> Result<FieldKey, String> {
    let mut k = Zeroizing::new([0u8; 32]);
    getrandom::fill(k.as_mut()).map_err(|e| e.to_string())?;
    Ok(k)
}

pub fn seal(key: &[u8; 32], label: &str, plain: &[u8]) -> Result<Vec<u8>, String> {
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
    let body = ChaCha20Poly1305::new(Key::from_slice(key))
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: label.as_bytes(),
            },
        )
        .map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(12 + body.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&body);
    Ok(out)
}

/// `None` when the key is wrong or the value was changed.
pub fn open(key: &[u8; 32], label: &str, data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 12 + 16 {
        return None;
    }
    ChaCha20Poly1305::new(Key::from_slice(key))
        .decrypt(
            Nonce::from_slice(&data[..12]),
            Payload {
                msg: &data[12..],
                aad: label.as_bytes(),
            },
        )
        .ok()
}

pub(crate) fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub(crate) fn from_hex(s: &str) -> Option<FieldKey> {
    if s.len() != 64 || !s.is_ascii() {
        return None;
    }
    let mut k = Zeroizing::new([0u8; 32]);
    for (i, slot) in k.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_binds_the_label() {
        let k = new_key().unwrap();
        let sealed = seal(&k, "command", b"rsync -av a b").unwrap();
        assert!(!sealed.windows(5).any(|w| w == b"rsync"));
        assert_eq!(open(&k, "command", &sealed).unwrap(), b"rsync -av a b");
        assert!(open(&k, "host", &sealed).is_none());
        assert!(open(&new_key().unwrap(), "command", &sealed).is_none());
        let mut bad = sealed.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(open(&k, "command", &bad).is_none());
        assert!(open(&k, "command", b"short").is_none());
    }

    #[test]
    fn hex_round_trips() {
        let k = new_key().unwrap();
        assert_eq!(*from_hex(&to_hex(k.as_ref())).unwrap(), *k);
        assert!(from_hex("zz").is_none());
    }
}
