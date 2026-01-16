use base64::engine::general_purpose::STANDARD_NO_PAD;
use base64::Engine;

use crate::error::{PostUrbitError, Result};

const CROCKFORD_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

pub fn base64_encode(data: &[u8]) -> String {
    STANDARD_NO_PAD.encode(data)
}

pub fn base64_decode(input: &str) -> Result<Vec<u8>> {
    if input.contains('=') {
        return Err(PostUrbitError::InvalidEncoding("base64 padding not allowed"));
    }
    STANDARD_NO_PAD
        .decode(input.as_bytes())
        .map_err(|_| PostUrbitError::InvalidEncoding("invalid base64"))
}

pub fn crockford_base32_encode(data: &[u8]) -> String {
    let mut result = String::new();
    let total_bits = data.len() * 8;
    let groups = (total_bits + 4) / 5;

    for group in 0..groups {
        let mut value: u8 = 0;
        for bit in 0..5 {
            let bit_index = group * 5 + bit;
            let bit_value = if bit_index < total_bits {
                let byte = data[bit_index / 8];
                let shift = 7 - (bit_index % 8);
                (byte >> shift) & 1
            } else {
                0
            };
            value = (value << 1) | bit_value;
        }
        result.push(CROCKFORD_ALPHABET[value as usize] as char);
    }

    result
}

pub fn crockford_base32_decode(input: &str) -> Result<Vec<u8>> {
    let mut buffer: u32 = 0;
    let mut bits: u8 = 0;
    let mut out = Vec::new();

    for ch in input.bytes() {
        let val = crockford_value(ch)?;
        buffer = (buffer << 5) | val as u32;
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }

    Ok(out)
}

pub fn validate_crockford_base32_lower(input: &str) -> Result<()> {
    if input.is_empty() {
        return Err(PostUrbitError::InvalidEncoding("empty base32"));
    }
    for ch in input.bytes() {
        if !(b'0'..=b'9').contains(&ch)
            && !(b'a'..=b'h').contains(&ch)
            && !(b'j'..=b'k').contains(&ch)
            && !(b'm'..=b'n').contains(&ch)
            && !(b'p'..=b't').contains(&ch)
            && !(b'v'..=b'z').contains(&ch)
        {
            return Err(PostUrbitError::InvalidEncoding("invalid base32 character"));
        }
    }
    Ok(())
}

fn crockford_value(ch: u8) -> Result<u8> {
    match ch {
        b'0'..=b'9' => Ok(ch - b'0'),
        b'a'..=b'h' => Ok(10 + (ch - b'a')),
        b'j' => Ok(18),
        b'k' => Ok(19),
        b'm' => Ok(20),
        b'n' => Ok(21),
        b'p' => Ok(22),
        b'q' => Ok(23),
        b'r' => Ok(24),
        b's' => Ok(25),
        b't' => Ok(26),
        b'v' => Ok(27),
        b'w' => Ok(28),
        b'x' => Ok(29),
        b'y' => Ok(30),
        b'z' => Ok(31),
        _ => Err(PostUrbitError::InvalidEncoding("invalid base32 character")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crockford_base32_round_trip() {
        let data = b"hello world";
        let encoded = crockford_base32_encode(data);
        validate_crockford_base32_lower(&encoded).unwrap();
        let decoded = crockford_base32_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base64_round_trip_no_padding() {
        let data = b"post-urbit";
        let encoded = base64_encode(data);
        assert!(!encoded.contains('='));
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base64_rejects_padding() {
        let err = base64_decode("Zg==").unwrap_err();
        assert!(matches!(err, PostUrbitError::InvalidEncoding(_)));
    }
}
