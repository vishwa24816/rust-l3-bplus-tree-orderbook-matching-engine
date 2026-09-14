#[repr(C)]
#[derive(Debug, Clone)]
pub struct TxHeader {
    pub nonce: u64,
    pub gas_limit: u64,
    pub gas_price: u64,
    pub to: [u8; 20],
    pub value: [u8; 16],
    pub data_len: u16,
}

impl TxHeader {
    pub const SIZE: usize = 62;
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub header: TxHeader,
    pub data: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("buffer too short: need at least 62 bytes")]
    TooShort,
    #[error("invalid data length: declared {declared}, available {available}")]
    InvalidDataLength { declared: u16, available: usize },
    #[error("invalid signature length: {0}")]
    InvalidSignatureLength(usize),
    #[error("gas limit exceeds maximum")]
    GasLimitExceeded,
    #[error("zero gas price")]
    ZeroGasPrice,
}

pub const MAX_GAS_LIMIT: u64 = 30_000_000;

pub fn decode_transaction(bytes: &[u8]) -> Result<Transaction, DecodeError> {
    if bytes.len() < TxHeader::SIZE {
        return Err(DecodeError::TooShort);
    }

    // Manual parse — avoids zerocopy alignment issues on unaligned slices.
    let h = &bytes[..TxHeader::SIZE];
    let header = TxHeader {
        nonce: u64::from_le_bytes(h[0..8].try_into().unwrap()),
        gas_limit: u64::from_le_bytes(h[8..16].try_into().unwrap()),
        gas_price: u64::from_le_bytes(h[16..24].try_into().unwrap()),
        to: h[24..44].try_into().unwrap(),
        value: h[44..60].try_into().unwrap(),
        data_len: u16::from_le_bytes(h[60..62].try_into().unwrap()),
    };

    if header.gas_limit > MAX_GAS_LIMIT {
        return Err(DecodeError::GasLimitExceeded);
    }
    if header.gas_price == 0 {
        return Err(DecodeError::ZeroGasPrice);
    }

    let data_end = TxHeader::SIZE + header.data_len as usize;
    if data_end > bytes.len() {
        return Err(DecodeError::InvalidDataLength {
            declared: header.data_len,
            available: bytes.len() - TxHeader::SIZE,
        });
    }

    let data = bytes[TxHeader::SIZE..data_end].to_vec();
    let signature = bytes[data_end..].to_vec();

    if !signature.is_empty() && signature.len() != 64 && signature.len() != 65 {
        return Err(DecodeError::InvalidSignatureLength(signature.len()));
    }

    Ok(Transaction { header, data, signature })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_header(nonce: u64, gas_limit: u64, gas_price: u64, to: [u8; 20], value: [u8; 16], data_len: u16) -> [u8; 62] {
        let mut buf = [0u8; 62];
        buf[0..8].copy_from_slice(&nonce.to_le_bytes());
        buf[8..16].copy_from_slice(&gas_limit.to_le_bytes());
        buf[16..24].copy_from_slice(&gas_price.to_le_bytes());
        buf[24..44].copy_from_slice(&to);
        buf[44..60].copy_from_slice(&value);
        buf[60..62].copy_from_slice(&data_len.to_le_bytes());
        buf
    }

    #[test]
    fn test_too_short() {
        assert!(decode_transaction(&[0u8; 10]).is_err());
    }

    #[test]
    fn test_zero_length_valid() {
        let header = make_header(1, 21000, 1, [0u8; 20], [0u8; 16], 0);
        let tx = decode_transaction(&header).unwrap();
        assert_eq!(tx.header.nonce, 1);
        assert!(tx.data.is_empty());
        assert!(tx.signature.is_empty());
    }

    #[test]
    fn test_with_data_and_signature() {
        let header = make_header(1, 21000, 1, [0u8; 20], [0u8; 16], 3);
        let mut bytes = header.to_vec();
        bytes.extend_from_slice(&[0xaa, 0xbb, 0xcc]); // data
        bytes.extend_from_slice(&[0u8; 64]); // signature
        let tx = decode_transaction(&bytes).unwrap();
        assert_eq!(tx.data, vec![0xaa, 0xbb, 0xcc]);
        assert_eq!(tx.signature.len(), 64);
    }

    #[test]
    fn test_gas_limit_exceeded() {
        let header = make_header(0, MAX_GAS_LIMIT + 1, 1, [0u8; 20], [0u8; 16], 0);
        assert!(matches!(decode_transaction(&header), Err(DecodeError::GasLimitExceeded)));
    }

    #[test]
    fn test_zero_gas_price() {
        let header = make_header(0, 21000, 0, [0u8; 20], [0u8; 16], 0);
        assert!(matches!(decode_transaction(&header), Err(DecodeError::ZeroGasPrice)));
    }

    #[test]
    fn test_invalid_data_length() {
        let header = make_header(0, 21000, 1, [0u8; 20], [0u8; 16], 10);
        let mut bytes = header.to_vec();
        bytes.extend_from_slice(&[0xaa; 5]); // only 5 bytes, declared 10
        assert!(matches!(decode_transaction(&bytes), Err(DecodeError::InvalidDataLength { .. })));
    }

    #[test]
    fn test_invalid_signature_length() {
        let header = make_header(0, 21000, 1, [0u8; 20], [0u8; 16], 0);
        let mut bytes = header.to_vec();
        bytes.extend_from_slice(&[0u8; 33]); // 33 bytes, not 64 or 65
        assert!(matches!(decode_transaction(&bytes), Err(DecodeError::InvalidSignatureLength(33))));
    }
}
