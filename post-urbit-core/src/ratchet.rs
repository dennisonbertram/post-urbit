use std::collections::HashMap;

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

pub fn two_dh_responder(
    identity_private: &StaticSecret,
    peer_identity_public: &PublicKey,
    peer_ephemeral_public: &PublicKey,
) -> ([u8; 32], [u8; 32]) {
    let dh1 = identity_private
        .diffie_hellman(peer_identity_public)
        .to_bytes();
    let dh2 = identity_private
        .diffie_hellman(peer_ephemeral_public)
        .to_bytes();
    (dh1, dh2)
}

#[derive(Clone)]
pub struct RatchetKeyPair {
    pub private: StaticSecret,
    pub public: PublicKey,
}

#[derive(Debug, Clone)]
pub struct ReceivingChain {
    pub chain_key: [u8; 32],
    pub chain_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SkippedKeyId {
    dh_public: [u8; 32],
    n: u32,
}

#[derive(Clone)]
pub struct RatchetState {
    pub peer_iid: [u8; 20],
    pub dh_sending_key: RatchetKeyPair,
    pub dh_receiving_key: PublicKey,
    pub root_key: [u8; 32],
    pub sending_chain_key: Option<[u8; 32]>,
    pub sending_chain_index: u32,
    pub previous_sending_chain_length: u32,
    pub receiving_chains: HashMap<[u8; 32], ReceivingChain>,
    skipped_keys: HashMap<SkippedKeyId, [u8; 32]>,
    pub max_skip: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatchetHeader {
    pub dh_public: [u8; 32],
    pub pn: u32,
    pub n: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialMessage {
    pub ephemeral_public: [u8; 32],
    pub message_key: [u8; 32],
}

impl RatchetState {
    pub fn initialize_initiator(
        root_key: [u8; 32],
        initial_chain_key: [u8; 32],
        peer_identity_key: PublicKey,
        peer_iid: [u8; 20],
        ephemeral_private: StaticSecret,
    ) -> Self {
        let ephemeral_public = PublicKey::from(&ephemeral_private);
        Self {
            peer_iid,
            dh_sending_key: RatchetKeyPair {
                private: ephemeral_private,
                public: ephemeral_public,
            },
            dh_receiving_key: peer_identity_key,
            root_key,
            sending_chain_key: Some(initial_chain_key),
            sending_chain_index: 0,
            previous_sending_chain_length: 0,
            receiving_chains: HashMap::new(),
            skipped_keys: HashMap::new(),
            max_skip: 100,
        }
    }

    pub fn initialize_responder(
        root_key: [u8; 32],
        initial_chain_key: [u8; 32],
        peer_ephemeral_key: PublicKey,
        peer_iid: [u8; 20],
        sending_private: StaticSecret,
    ) -> Self {
        let sending_public = PublicKey::from(&sending_private);
        let mut receiving_chains = HashMap::new();
        receiving_chains.insert(
            peer_ephemeral_key.to_bytes(),
            ReceivingChain {
                chain_key: initial_chain_key,
                chain_index: 0,
            },
        );
        Self {
            peer_iid,
            dh_sending_key: RatchetKeyPair {
                private: sending_private,
                public: sending_public,
            },
            dh_receiving_key: peer_ephemeral_key,
            root_key,
            sending_chain_key: None,
            sending_chain_index: 0,
            previous_sending_chain_length: 0,
            receiving_chains,
            skipped_keys: HashMap::new(),
            max_skip: 100,
        }
    }

    pub fn initial_message_key(&mut self) -> Result<InitialMessage> {
        let chain_key = self
            .sending_chain_key
            .ok_or(PostUrbitError::InvalidInput("missing sending chain"))?;
        let (new_chain, message_key) = kdf_chain_step(&chain_key);
        self.sending_chain_key = Some(new_chain);
        let n = self.sending_chain_index;
        if n == 0 {
            self.sending_chain_index = 1;
        } else {
            self.sending_chain_index += 1;
        }
        Ok(InitialMessage {
            ephemeral_public: self.dh_sending_key.public.to_bytes(),
            message_key,
        })
    }

    pub fn initial_receive_message_key(&mut self) -> Result<[u8; 32]> {
        let key = self.dh_receiving_key.to_bytes();
        let chain = self
            .receiving_chains
            .get_mut(&key)
            .ok_or(PostUrbitError::InvalidInput("missing receiving chain"))?;
        let (new_chain, message_key) = kdf_chain_step(&chain.chain_key);
        chain.chain_key = new_chain;
        chain.chain_index += 1;
        Ok(message_key)
    }

    pub fn ratchet_encrypt(&mut self) -> Result<(RatchetHeader, [u8; 32])> {
        if self.sending_chain_key.is_none() {
            self.previous_sending_chain_length = self.sending_chain_index;
            let new_private = StaticSecret::random_from_rng(rand::rngs::OsRng);
            let new_public = PublicKey::from(&new_private);
            self.dh_sending_key = RatchetKeyPair {
                private: new_private,
                public: new_public,
            };
            let dh_output = self
                .dh_sending_key
                .private
                .diffie_hellman(&self.dh_receiving_key)
                .to_bytes();
            let (new_root, chain_key) = kdf_root(&self.root_key, &dh_output)?;
            self.root_key = new_root;
            self.sending_chain_key = Some(chain_key);
            self.sending_chain_index = 0;
        }

        let chain_key = self
            .sending_chain_key
            .ok_or(PostUrbitError::InvalidInput("missing sending chain"))?;
        let n = self.sending_chain_index;
        let (new_chain, message_key) = kdf_chain_step(&chain_key);
        self.sending_chain_key = Some(new_chain);
        self.sending_chain_index += 1;

        let header = RatchetHeader {
            dh_public: self.dh_sending_key.public.to_bytes(),
            pn: self.previous_sending_chain_length,
            n,
        };
        Ok((header, message_key))
    }

    pub fn ratchet_decrypt(&mut self, header: &RatchetHeader) -> Result<[u8; 32]> {
        if let Some(key) = self
            .skipped_keys
            .remove(&SkippedKeyId { dh_public: header.dh_public, n: header.n })
        {
            return Ok(key);
        }

        if header.dh_public != self.dh_receiving_key.to_bytes() {
            let current = self.dh_receiving_key.to_bytes();
            self.skip_message_keys(&current, header.pn)?;

            self.dh_receiving_key = PublicKey::from(header.dh_public);
            let dh_output = self
                .dh_sending_key
                .private
                .diffie_hellman(&self.dh_receiving_key)
                .to_bytes();
            let (new_root, receiving_chain_key) = kdf_root(&self.root_key, &dh_output)?;
            self.root_key = new_root;
            self.sending_chain_key = None;

            self.receiving_chains.insert(
                header.dh_public,
                ReceivingChain {
                    chain_key: receiving_chain_key,
                    chain_index: 0,
                },
            );
        }

        let chain = self
            .receiving_chains
            .get_mut(&header.dh_public)
            .ok_or(PostUrbitError::InvalidInput("missing receiving chain"))?;

        while chain.chain_index < header.n {
            if self.skipped_keys.len() as u32 >= self.max_skip {
                return Err(PostUrbitError::InvalidInput("too many skipped"));
            }
            let (new_chain, message_key) = kdf_chain_step(&chain.chain_key);
            let key_id = SkippedKeyId {
                dh_public: header.dh_public,
                n: chain.chain_index,
            };
            self.skipped_keys.insert(key_id, message_key);
            chain.chain_key = new_chain;
            chain.chain_index += 1;
        }

        let (new_chain, message_key) = kdf_chain_step(&chain.chain_key);
        chain.chain_key = new_chain;
        chain.chain_index += 1;
        Ok(message_key)
    }

    fn skip_message_keys(&mut self, dh_public: &[u8; 32], until: u32) -> Result<()> {
        let Some(chain) = self.receiving_chains.get_mut(dh_public) else {
            return Ok(());
        };
        while chain.chain_index < until {
            if self.skipped_keys.len() as u32 >= self.max_skip {
                return Err(PostUrbitError::InvalidInput("too many skipped"));
            }
            let (new_chain, message_key) = kdf_chain_step(&chain.chain_key);
            let key_id = SkippedKeyId {
                dh_public: *dh_public,
                n: chain.chain_index,
            };
            self.skipped_keys.insert(key_id, message_key);
            chain.chain_key = new_chain;
            chain.chain_index += 1;
        }
        Ok(())
    }
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

    #[test]
    fn kdf_initial_responder_matches_initiator() {
        let alice_priv_bytes: [u8; 32] = hex::decode(
            "7ff8c1a741fd3c5253f5d6953cd78f5411f36507f8f653b498e19d381bf7877b",
        )
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();
        let alice_priv = StaticSecret::from(alice_priv_bytes);
        let alice_pub = PublicKey::from(&alice_priv);

        let alice_ephemeral_bytes: [u8; 32] = hex::decode(
            "3803e7c7f979da62ad5f1aaf9253be156695d8ae845b8cbc2e24afcd9a32d50d",
        )
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();
        let alice_ephemeral = StaticSecret::from(alice_ephemeral_bytes);
        let alice_ephemeral_pub = PublicKey::from(&alice_ephemeral);

        let bob_priv_bytes: [u8; 32] = hex::decode(
            "ea7d6a9217038a4c58f81cfe00b87f1c4feeaa3f182d430936646c4cd11885b2",
        )
        .unwrap()
        .as_slice()
        .try_into()
        .unwrap();
        let bob_priv = StaticSecret::from(bob_priv_bytes);
        let bob_pub = PublicKey::from(&bob_priv);

        let (dh1_a, dh2_a) = two_dh_initiator(&alice_priv, &alice_ephemeral, &bob_pub);
        let (dh1_b, dh2_b) = two_dh_responder(&bob_priv, &alice_pub, &alice_ephemeral_pub);
        assert_eq!(dh1_a, dh1_b);
        assert_eq!(dh2_a, dh2_b);
    }

    #[test]
    fn ratchet_initial_and_out_of_order() {
        let alice_identity = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let bob_identity = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let alice_ephemeral = StaticSecret::random_from_rng(rand::rngs::OsRng);

        let bob_pub = PublicKey::from(&bob_identity);
        let alice_pub = PublicKey::from(&alice_identity);
        let alice_ephemeral_pub = PublicKey::from(&alice_ephemeral);

        let (dh1, dh2) = two_dh_initiator(&alice_identity, &alice_ephemeral, &bob_pub);
        let alice_iid = [1u8; 20];
        let bob_iid = [2u8; 20];
        let (root_a, chain_a) = kdf_initial(&dh1, &dh2, &alice_iid, &bob_iid).unwrap();
        let mut alice_state = RatchetState::initialize_initiator(
            root_a,
            chain_a,
            bob_pub,
            bob_iid,
            alice_ephemeral,
        );

        let (dh1_b, dh2_b) = two_dh_responder(&bob_identity, &alice_pub, &alice_ephemeral_pub);
        let (root_b, chain_b) = kdf_initial(&dh1_b, &dh2_b, &alice_iid, &bob_iid).unwrap();
        let bob_sending = StaticSecret::random_from_rng(rand::rngs::OsRng);
        let mut bob_state = RatchetState::initialize_responder(
            root_b,
            chain_b,
            alice_ephemeral_pub,
            alice_iid,
            bob_sending,
        );

        let initial = alice_state.initial_message_key().unwrap();
        let initial_recv = bob_state.initial_receive_message_key().unwrap();
        assert_eq!(initial.message_key, initial_recv);

        let (h1, k1) = alice_state.ratchet_encrypt().unwrap();
        let (h2, k2) = alice_state.ratchet_encrypt().unwrap();

        let k2_recv = bob_state.ratchet_decrypt(&h2).unwrap();
        assert_eq!(k2, k2_recv);
        let k1_recv = bob_state.ratchet_decrypt(&h1).unwrap();
        assert_eq!(k1, k1_recv);
    }
}
