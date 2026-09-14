use sha2::{Sha256, Digest};
use secp256k1::{Secp256k1, Message, ecdsa::RecoveryId};

use crate::types::Transaction;

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("missing signature")]
    MissingSignature,
    #[error("invalid signature length: expected 64 or 65, got {0}")]
    InvalidLength(usize),
    #[error("invalid recovery id")]
    InvalidRecoveryId,
    #[error("signature verification failed")]
    VerificationFailed,
}

pub fn verify_signature(tx: &Transaction) -> Result<[u8; 20], VerifyError> {
    if tx.signature.is_empty() {
        return Err(VerifyError::MissingSignature);
    }
    if tx.signature.len() != 64 && tx.signature.len() != 65 {
        return Err(VerifyError::InvalidLength(tx.signature.len()));
    }

    let mut hasher = Sha256::new();
    hasher.update(&tx.header.nonce.to_le_bytes());
    hasher.update(&tx.header.gas_limit.to_le_bytes());
    hasher.update(&tx.header.gas_price.to_le_bytes());
    hasher.update(&tx.header.to);
    hasher.update(&tx.header.value);
    hasher.update(&tx.data);
    let hash = hasher.finalize();

    let secp = Secp256k1::verification_only();
    let message = Message::from_digest_slice(&hash).expect("32 bytes");

    let (recovery_id, signature) = if tx.signature.len() == 65 {
        let v = tx.signature[64];
        let rid = RecoveryId::from_i32(v as i32).map_err(|_| VerifyError::InvalidRecoveryId)?;
        (rid, &tx.signature[..64])
    } else {
        (RecoveryId::from_i32(0).map_err(|_| VerifyError::InvalidRecoveryId)?, &tx.signature[..])
    };

    let sig = secp256k1::ecdsa::RecoverableSignature::from_compact(signature, recovery_id)
        .map_err(|_| VerifyError::VerificationFailed)?;

    let pubkey = secp.recover_ecdsa(&message, &sig)
        .map_err(|_| VerifyError::VerificationFailed)?;

    let mut sender = [0u8; 20];
    let uncompressed = pubkey.serialize_uncompressed();
    let full_hash = Sha256::digest(&uncompressed[1..]);
    sender.copy_from_slice(&full_hash[12..]);

    Ok(sender)
}
