use hmac::{Hmac, Mac};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::error::{PostUrbitError, Result};

const RATCHET_INFO: &[u8] = b"post-urbit-ratchet-v1";
const X3DH_INFO: &[u8] = b"post-urbit-x3dh-v1";
const SENDER_KEY_INFO: &[u8] = b"post-urbit-sender-key-v1:";

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

pub fn kdf_initial(
    dh1: &[u8; 32],
    dh2: &[u8; 32],
    iid_a: &[u8; 20],
    iid_b: &[u8; 20],
) -> Result<([u8; 32], [u8; 32])> {
    let mut ikm = [0u8; 64];
    ikm[..32].copy_from_slice(dh1);
    ikm[32..].copy_from_slice(dh2);

    let salt = if iid_a < iid_b {
        let mut out = [0u8; 40];
        out[..20].copy_from_slice(iid_a);
        out[20..].copy_from_slice(iid_b);
        out
    } else {
        let mut out = [0u8; 40];
        out[..20].copy_from_slice(iid_b);
        out[20..].copy_from_slice(iid_a);
        out
    };

    let hk = Hkdf::<Sha256>::new(Some(&salt), &ikm);
    let mut out = [0u8; 64];
    hk.expand(X3DH_INFO, &mut out)
        .map_err(|_| PostUrbitError::Crypto("x3dh hkdf"))?;

    let mut root_key = [0u8; 32];
    let mut chain_key = [0u8; 32];
    root_key.copy_from_slice(&out[..32]);
    chain_key.copy_from_slice(&out[32..]);
    Ok((root_key, chain_key))
}

pub fn kdf_sender_key(
    chain_key: &[u8; 32],
    group_id: &[u8; 20],
    sender_iid: &[u8; 20],
    key_id: &[u8; 16],
) -> ([u8; 32], [u8; 32]) {
    let mut info = Vec::with_capacity(SENDER_KEY_INFO.len() + 20 + 1 + 20 + 1 + 16);
    info.extend_from_slice(SENDER_KEY_INFO);
    info.extend_from_slice(group_id);
    info.push(b':');
    info.extend_from_slice(sender_iid);
    info.push(b':');
    info.extend_from_slice(key_id);

    let mut message_data = Vec::with_capacity(1 + info.len());
    message_data.push(0x01);
    message_data.extend_from_slice(&info);
    let message_key = hmac_sha256(chain_key, &message_data);

    let mut chain_data = Vec::with_capacity(1 + info.len());
    chain_data.push(0x02);
    chain_data.extend_from_slice(&info);
    let new_chain_key = hmac_sha256(chain_key, &chain_data);

    (new_chain_key, message_key)
}

pub fn two_dh_initiator(
    identity_private: &StaticSecret,
    ephemeral_private: &StaticSecret,
    peer_identity_public: &PublicKey,
) -> ([u8; 32], [u8; 32]) {
    let dh1 = identity_private
        .diffie_hellman(peer_identity_public)
        .to_bytes();
    let dh2 = ephemeral_private
        .diffie_hellman(peer_identity_public)
        .to_bytes();
    (dh1, dh2)
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

    #[test]
    fn kdf_chain_step_matches_test_vector() {
        let chain = hex::decode(
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        )
        .unwrap();
        let chain: [u8; 32] = chain.as_slice().try_into().unwrap();
        let (new_chain, message) = kdf_chain_step(&chain);
        assert_eq!(
            hex::encode(message),
            "9b4c8120a4823a95f47cde17a244f4507244ee6e3957d1fab9fa29b44d3829b7"
        );
        assert_eq!(
            hex::encode(new_chain),
            "4304c22c84a53755ab08ead8d97a8d429be5efa480682d7ad1da27f73e1fbe1d"
        );
    }

    #[test]
    fn kdf_root_matches_test_vector() {
        let root = hex::decode(
            "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f",
        )
        .unwrap();
        let root: [u8; 32] = root.as_slice().try_into().unwrap();
        let dh_output = hex::decode(
            "1f1e1d1c1b1a191817161514131211100f0e0d0c0b0a09080706050403020100",
        )
        .unwrap();
        let (new_root, new_chain) = kdf_root(&root, &dh_output).unwrap();
        assert_eq!(
            hex::encode(new_root),
            "76b6f7be00a618e3cd626650dc9b3c70f044b12499f2ffb94ca72c7fb08f0fb5"
        );
        assert_eq!(
            hex::encode(new_chain),
            "96c7dbc35d738c6d1729e2cf160f12ee8cc045540836c8b67c18d843ee710d74"
        );
    }

    #[test]
    fn kdf_initial_matches_test_vector() {
        let ik_a_bytes: [u8; 32] = hex::decode(
            "7ff8c1a741fd3c5253f5d6953cd78f5411f36507f8f653b498e19d381bf7877b",
        )
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();
        let ik_a = StaticSecret::from(ik_a_bytes);

        let ek_a_bytes: [u8; 32] = hex::decode(
            "3803e7c7f979da62ad5f1aaf9253be156695d8ae845b8cbc2e24afcd9a32d50d",
        )
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();
        let ek_a = StaticSecret::from(ek_a_bytes);

        let ik_b_pub_bytes: [u8; 32] = hex::decode(
            "e473a89c43f80e7f3702c9ee7984104879474aa53b72b4e4c8e2b79d0f78a84e",
        )
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();
        let ik_b_pub = PublicKey::from(ik_b_pub_bytes);

        let (dh1, dh2) = two_dh_initiator(&ik_a, &ek_a, &ik_b_pub);
        assert_eq!(
            hex::encode(dh1),
            "858d12b60f9a452f4f0925b669236bf96492d4dfb68b8ad9a4b0c34249db4f1a"
        );
        assert_eq!(
            hex::encode(dh2),
            "31548fcb50ec70e48a1dda37f3e1ea13cee05b5f55ffaa34e88804ff55d8ac5d"
        );

        let alice_iid: [u8; 20] = hex::decode("586a763f2c82b31a0c5de9dcaef01e0261e0785b")
            .unwrap()
            .as_slice()
            .try_into()
            .unwrap();
        let bob_iid: [u8; 20] = hex::decode("d15c5160257b140ed4bf313fbf92eef8a266de56")
            .unwrap()
            .as_slice()
            .try_into()
            .unwrap();

        let (root_key, chain_key) = kdf_initial(&dh1, &dh2, &alice_iid, &bob_iid).unwrap();
        assert_eq!(
            hex::encode(root_key),
            "dc32bc7298c8558b3e347cad9196a2a9f1744185be574ea869e441716eb7420d"
        );
        assert_eq!(
            hex::encode(chain_key),
            "47920ff7fbbdca074b8abebfc125e456909b36635c9177a8afee8a1e6314d86e"
        );
    }
}
