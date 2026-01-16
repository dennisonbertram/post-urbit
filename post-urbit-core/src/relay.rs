use crate::error::{PostUrbitError, Result};

const PURL_MAGIC: &[u8; 4] = b"PURL";
const PURL_VERSION: u8 = 1;

#[derive(Debug, Clone)]
pub struct PurlPacket {
    pub packet_type: u8,
    pub allocation_token: [u8; 16],
    pub destination_iid: [u8; 20],
    pub payload: Vec<u8>,
}

pub fn encode_purl(packet: &PurlPacket) -> Result<Vec<u8>> {
    let len: u16 = packet
        .payload
        .len()
        .try_into()
        .map_err(|_| PostUrbitError::InvalidInput("purl payload length"))?;
    let mut out = Vec::with_capacity(4 + 1 + 1 + 16 + 20 + 2 + packet.payload.len());
    out.extend_from_slice(PURL_MAGIC);
    out.push(PURL_VERSION);
    out.push(packet.packet_type);
    out.extend_from_slice(&packet.allocation_token);
    out.extend_from_slice(&packet.destination_iid);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&packet.payload);
    Ok(out)
}

pub fn decode_purl(bytes: &[u8]) -> Result<PurlPacket> {
    if bytes.len() < 4 + 1 + 1 + 16 + 20 + 2 {
        return Err(PostUrbitError::InvalidInput("purl length"));
    }
    if &bytes[..4] != PURL_MAGIC {
        return Err(PostUrbitError::InvalidInput("purl magic"));
    }
    if bytes[4] != PURL_VERSION {
        return Err(PostUrbitError::InvalidInput("purl version"));
    }
    let packet_type = bytes[5];
    let mut idx = 6;

    let mut allocation_token = [0u8; 16];
    allocation_token.copy_from_slice(&bytes[idx..idx + 16]);
    idx += 16;

    let mut destination_iid = [0u8; 20];
    destination_iid.copy_from_slice(&bytes[idx..idx + 20]);
    idx += 20;

    let payload_len = u16::from_be_bytes([bytes[idx], bytes[idx + 1]]) as usize;
    idx += 2;
    if bytes.len() != idx + payload_len {
        return Err(PostUrbitError::InvalidInput("purl payload length"));
    }
    let payload = bytes[idx..].to_vec();

    Ok(PurlPacket {
        packet_type,
        allocation_token,
        destination_iid,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purl_round_trip() {
        let packet = PurlPacket {
            packet_type: 0x01,
            allocation_token: [7u8; 16],
            destination_iid: [9u8; 20],
            payload: vec![1, 2, 3],
        };
        let encoded = encode_purl(&packet).unwrap();
        let decoded = decode_purl(&encoded).unwrap();
        assert_eq!(decoded.packet_type, 0x01);
        assert_eq!(decoded.payload, vec![1, 2, 3]);
    }

    #[test]
    fn purl_rejects_bad_magic() {
        let err = decode_purl(b"PURX").unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidInput(_)));
    }
}
