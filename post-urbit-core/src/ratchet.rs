use hmac::{Hmac, Mac};
use hkdf::Hkdf;
use sha2::Sha256;

use crate::error::{PostUrbitError, Result};

const RATCHET_INFO: &[u8] = b"post-urbit-ratchet-v1";

type HmacSha256 = Hmac<Sha256>;

pub fn kdf_chain_step(chain_key: &[u8; 32]) -> ([u8; 32], [u8; 32]) {
    let message_key = hmac_sha256(chain_key, &[0x01]);
    let new_chain_key = hmac_sha256(chain_key, &[0x02]);
    (new_chain_key, message_key)
}

pub fn kdf_root(root_key: &[u8; 32], dh_output: &[u8]) -> Result<([u8; 32], [u8; 32])> {
    let hk = Hkdf::<Sha256>::new(Some(root_key), dh_output);
    let mut out = [0u8; 64];
    hk.expand(RATCHET_INFO, &mut out)
        .map_err(|_| PostUrbitError::Crypto("ratchet hkdf"))?;
    let mut new_root = [0u8; 32];
    let mut chain_key = [0u8; 32];
    new_root.copy_from_slice(&out[..32]);
    chain_key.copy_from_slice(&out[32..]);
    Ok((new_root, chain_key))
}

fn hmac_sha256(key: &[u8; 32], data: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac key");
    mac.update(data);
    let result = mac.finalize().into_bytes();
    result.as_slice().try_into().expect("hmac length")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kdf_chain_step_lengths() {
        let key = [7u8; 32];
        let (next, msg) = kdf_chain_step(&key);
        assert_eq!(next.len(), 32);
        assert_eq!(msg.len(), 32);
    }

    #[test]
    fn kdf_root_lengths() {
        let root = [9u8; 32];
        let (new_root, chain) = kdf_root(&root, b"input").unwrap();
        assert_eq!(new_root.len(), 32);
        assert_eq!(chain.len(), 32);
    }
}
