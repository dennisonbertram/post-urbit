use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::ChaCha20Poly1305;
use chacha20poly1305::KeyInit;
use ed25519_dalek::{Signature, SigningKey, VerifyingKey, Signer};

use crate::encoding::{base64_decode, base64_encode};
use crate::error::{PostUrbitError, Result};

const PUSE_MAGIC: &[u8; 4] = b"PUSE";
const PUSE_VERSION: u8 = 1;

#[derive(Debug, Clone)]
pub struct PUSEHeader {
    pub flags: u8,
    pub sender_iid: [u8; 20],
    pub recipient_iid: [u8; 20],
    pub message_id: [u8; 16],
    pub header_extension: Vec<u8>,
    pub nonce: [u8; 12],
    pub ciphertext_length: u32,
}

#[derive(Debug, Clone)]
pub struct PUSEEnvelope {
    pub header: PUSEHeader,
    pub ciphertext: Vec<u8>,
    pub signature: [u8; 64],
}

pub fn encode_puse_header(header: &PUSEHeader) -> Result<Vec<u8>> {
    validate_header_extension(&header.header_extension)?;
    let ext_len: u16 = header
        .header_extension
        .len()
        .try_into()
        .map_err(|_| PostUrbitError::InvalidInput("header extension length"))?;

    let mut out = Vec::with_capacity(64 + header.header_extension.len());
    out.extend_from_slice(PUSE_MAGIC);
    out.push(PUSE_VERSION);
    out.push(header.flags);
    out.extend_from_slice(&header.sender_iid);
    out.extend_from_slice(&header.recipient_iid);
    out.extend_from_slice(&header.message_id);
    out.extend_from_slice(&ext_len.to_be_bytes());
    out.extend_from_slice(&header.header_extension);
    out.extend_from_slice(&header.nonce);
    out.extend_from_slice(&header.ciphertext_length.to_be_bytes());
    Ok(out)
}

pub fn decode_puse_envelope(bytes: &[u8]) -> Result<PUSEEnvelope> {
    if bytes.len() < 4 + 1 + 1 + 20 + 20 + 16 + 2 + 1 + 12 + 4 + 64 {
        return Err(PostUrbitError::InvalidInput("puse envelope too short"));
    }
    if &bytes[..4] != PUSE_MAGIC {
        return Err(PostUrbitError::InvalidInput("puse magic"));
    }
    if bytes[4] != PUSE_VERSION {
        return Err(PostUrbitError::InvalidInput("puse version"));
    }

    let mut idx = 5;
    let flags = bytes[idx];
    idx += 1;

    let mut sender_iid = [0u8; 20];
    sender_iid.copy_from_slice(&bytes[idx..idx + 20]);
    idx += 20;

    let mut recipient_iid = [0u8; 20];
    recipient_iid.copy_from_slice(&bytes[idx..idx + 20]);
    idx += 20;

    let mut message_id = [0u8; 16];
    message_id.copy_from_slice(&bytes[idx..idx + 16]);
    idx += 16;

    let ext_len = u16::from_be_bytes([
        bytes[idx],
        bytes[idx + 1],
    ]) as usize;
    idx += 2;

    if bytes.len() < idx + ext_len + 12 + 4 + 64 {
        return Err(PostUrbitError::InvalidInput("puse envelope length"));
    }

    let header_extension = bytes[idx..idx + ext_len].to_vec();
    idx += ext_len;
    validate_header_extension(&header_extension)?;

    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&bytes[idx..idx + 12]);
    idx += 12;

    let ciphertext_length = u32::from_be_bytes([
        bytes[idx],
        bytes[idx + 1],
        bytes[idx + 2],
        bytes[idx + 3],
    ]);
    idx += 4;

    let ct_len = ciphertext_length as usize;
    if bytes.len() < idx + ct_len + 64 {
        return Err(PostUrbitError::InvalidInput("puse ciphertext length"));
    }

    let ciphertext = bytes[idx..idx + ct_len].to_vec();
    idx += ct_len;

    let mut signature = [0u8; 64];
    signature.copy_from_slice(&bytes[idx..idx + 64]);
    idx += 64;

    if idx != bytes.len() {
        return Err(PostUrbitError::InvalidInput("puse trailing bytes"));
    }

    Ok(PUSEEnvelope {
        header: PUSEHeader {
            flags,
            sender_iid,
            recipient_iid,
            message_id,
            header_extension,
            nonce,
            ciphertext_length,
        },
        ciphertext,
        signature,
    })
}

pub fn encrypt_puse_payload(
    message_key: &[u8; 32],
    header_extension: &[u8],
    nonce: &[u8; 12],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(message_key.into());
    cipher
        .encrypt(
            nonce.into(),
            Payload {
                msg: plaintext,
                aad: header_extension,
            },
        )
        .map_err(|_| PostUrbitError::Crypto("puse encrypt"))
}

pub fn decrypt_puse_payload(
    message_key: &[u8; 32],
    header_extension: &[u8],
    nonce: &[u8; 12],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(message_key.into());
    cipher
        .decrypt(
            nonce.into(),
            Payload {
                msg: ciphertext,
                aad: header_extension,
            },
        )
        .map_err(|_| PostUrbitError::Crypto("puse decrypt"))
}

pub fn build_puse_envelope(
    signing_key: &SigningKey,
    mut header: PUSEHeader,
    message_key: &[u8; 32],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let ciphertext = encrypt_puse_payload(
        message_key,
        &header.header_extension,
        &header.nonce,
        plaintext,
    )?;
    header.ciphertext_length = ciphertext
        .len()
        .try_into()
        .map_err(|_| PostUrbitError::InvalidInput("ciphertext length"))?;

    let mut bytes = encode_puse_header(&header)?;
    bytes.extend_from_slice(&ciphertext);

    let signature: Signature = signing_key.sign(&bytes);
    bytes.extend_from_slice(&signature.to_bytes());
    Ok(bytes)
}

pub fn verify_puse_signature(envelope_bytes: &[u8], signing_keys: &[String]) -> Result<()> {
    if envelope_bytes.len() < 64 {
        return Err(PostUrbitError::InvalidInput("puse envelope too short"));
    }
    let signed_data_len = envelope_bytes.len() - 64;
    let signature_bytes = &envelope_bytes[signed_data_len..];
    let signature = Signature::from_bytes(
        signature_bytes
            .try_into()
            .map_err(|_| PostUrbitError::InvalidInput("signature length"))?,
    );

    for key in signing_keys {
        let key_bytes = base64_decode(key)?;
        if key_bytes.len() != 32 {
            continue;
        }
        let verifying_key = VerifyingKey::from_bytes(
            key_bytes
                .as_slice()
                .try_into()
                .map_err(|_| PostUrbitError::InvalidInput("signing key length"))?,
        )
        .map_err(|_| PostUrbitError::InvalidInput("signing key parse"))?;
        if verifying_key
            .verify_strict(&envelope_bytes[..signed_data_len], &signature)
            .is_ok()
        {
            return Ok(());
        }
    }

    Err(PostUrbitError::Crypto("puse signature invalid"))
}

fn validate_header_extension(extension: &[u8]) -> Result<()> {
    if extension.is_empty() {
        return Err(PostUrbitError::InvalidInput("header extension required"));
    }
    if extension.len() > 1024 {
        return Err(PostUrbitError::InvalidInput("header extension too large"));
    }
    match extension[0] {
        0x00 => {
            if extension.len() != 33 {
                return Err(PostUrbitError::InvalidInput("initial extension length"));
            }
        }
        0x01 => {
            if extension.len() != 41 {
                return Err(PostUrbitError::InvalidInput("ratchet extension length"));
            }
        }
        0x02 => {
            if extension.len() != 21 {
                return Err(PostUrbitError::InvalidInput("group extension length"));
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn base64_signature(signature: &[u8; 64]) -> String {
    base64_encode(signature)
}

pub fn decode_signature_b64(signature: &str) -> Result<[u8; 64]> {
    let bytes = base64_decode(signature)?;
    bytes
        .try_into()
        .map_err(|_| PostUrbitError::InvalidInput("signature length"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ratchet::kdf_chain_step;

    #[test]
    fn puse_header_round_trip() {
        let header = PUSEHeader {
            flags: 0,
            sender_iid: [1u8; 20],
            recipient_iid: [2u8; 20],
            message_id: [3u8; 16],
            header_extension: vec![0x00; 33],
            nonce: [4u8; 12],
            ciphertext_length: 16,
        };

        let mut envelope = encode_puse_header(&header).unwrap();
        envelope.extend_from_slice(&[0u8; 16]);
        envelope.extend_from_slice(&[0u8; 64]);

        let decoded = decode_puse_envelope(&envelope).unwrap();
        assert_eq!(decoded.header.sender_iid, header.sender_iid);
        assert_eq!(decoded.header.header_extension.len(), 33);
    }

    #[test]
    fn puse_encrypt_decrypt_round_trip() {
        let message_key = [7u8; 32];
        let nonce = [9u8; 12];
        let aad = vec![0x01; 41];
        let plaintext = b"hello";
        let ciphertext = encrypt_puse_payload(&message_key, &aad, &nonce, plaintext).unwrap();
        let decrypted = decrypt_puse_payload(&message_key, &aad, &nonce, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn puse_envelope_matches_test_vector_10() {
        let signing_seed = hex::decode(
            "033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40",
        )
        .unwrap();
        let signing_key = SigningKey::from_bytes(signing_seed.as_slice().try_into().unwrap());

        let sender_iid: [u8; 20] = hex::decode("586a763f2c82b31a0c5de9dcaef01e0261e0785b")
            .unwrap()
            .as_slice()
            .try_into()
            .unwrap();
        let recipient_iid: [u8; 20] =
            hex::decode("d15c5160257b140ed4bf313fbf92eef8a266de56")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let message_id: [u8; 16] =
            hex::decode("550e8400e29b41d4a716446655440000")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let header_extension = hex::decode(
            "0089fe87345d1c24ed5fc16df9080eef9345a824cddf37b5fec4be627904522217",
        )
        .unwrap();
        let nonce: [u8; 12] =
            hex::decode("6560a3c00102030405060708")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();

        let initial_chain_key = hex::decode(
            "47920ff7fbbdca074b8abebfc125e456909b36635c9177a8afee8a1e6314d86e",
        )
        .unwrap();
        let initial_chain_key: [u8; 32] = initial_chain_key.as_slice().try_into().unwrap();
        let (_new_chain, message_key) = kdf_chain_step(&initial_chain_key);

        let header = PUSEHeader {
            flags: 0,
            sender_iid,
            recipient_iid,
            message_id,
            header_extension,
            nonce,
            ciphertext_length: 0,
        };

        let envelope = build_puse_envelope(&signing_key, header, &message_key, b"hello").unwrap();
        assert_eq!(
            hex::encode(envelope),
            "505553450100586a763f2c82b31a0c5de9dcaef01e0261e0785bd15c5160257b140ed4bf313fbf92eef8a266de56550e8400e29b41d4a71644665544000000210089fe87345d1c24ed5fc16df9080eef9345a824cddf37b5fec4be6279045222176560a3c0010203040506070800000015900c9a179c3e847fdf3660033e1dc73ad0a11a8db6fdc884da4019717b56265c8172c731a3ea577fad6e77fb736f765a93d1cabfe6c2ca99a96620c3d0b60cf6f3c1ccaddfd1dddf8df197ad4e7f480ee513fec70d"
        );
    }

    #[test]
    fn puse_envelope_matches_test_vector_11() {
        let signing_seed = hex::decode(
            "033cb5927062653e49646945878c1a40c6c9ee4694c93c10886d45d320028f40",
        )
        .unwrap();
        let signing_key = SigningKey::from_bytes(signing_seed.as_slice().try_into().unwrap());

        let sender_iid: [u8; 20] = hex::decode("586a763f2c82b31a0c5de9dcaef01e0261e0785b")
            .unwrap()
            .as_slice()
            .try_into()
            .unwrap();
        let recipient_iid: [u8; 20] =
            hex::decode("d15c5160257b140ed4bf313fbf92eef8a266de56")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let message_id: [u8; 16] =
            hex::decode("550e8400e29b41d4a716446655440001")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();
        let header_extension = hex::decode(
            "0189fe87345d1c24ed5fc16df9080eef9345a824cddf37b5fec4be6279045222170000000000000001",
        )
        .unwrap();
        let nonce: [u8; 12] =
            hex::decode("6560a3c11112131415161718")
                .unwrap()
                .as_slice()
                .try_into()
                .unwrap();

        let chain_key_1 = hex::decode(
            "4e75e0384cbd36e42464b656a3a1f8078f4c72ac8a8eceba75e2eb21689cde91",
        )
        .unwrap();
        let chain_key_1: [u8; 32] = chain_key_1.as_slice().try_into().unwrap();
        let (_new_chain, message_key) = kdf_chain_step(&chain_key_1);

        let header = PUSEHeader {
            flags: 0,
            sender_iid,
            recipient_iid,
            message_id,
            header_extension,
            nonce,
            ciphertext_length: 0,
        };

        let envelope =
            build_puse_envelope(&signing_key, header, &message_key, b"hello again").unwrap();
        assert_eq!(
            hex::encode(envelope),
            "505553450100586a763f2c82b31a0c5de9dcaef01e0261e0785bd15c5160257b140ed4bf313fbf92eef8a266de56550e8400e29b41d4a71644665544000100290189fe87345d1c24ed5fc16df9080eef9345a824cddf37b5fec4be62790452221700000000000000016560a3c111121314151617180000001b32c8241cd1dd0baff3719c390843c0b056443cc1c0686b5f3c0126094b4d9c3ca5e0229d6f40a94b13492ff290bf812fbc203dcae818912457fc4befc0af1e857baab75d0ca434de46205b2f64262d1fed5f5963d33f43cb54c60c"
        );
    }
}
