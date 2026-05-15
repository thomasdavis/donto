//! Release envelope — M9 spike.
//!
//! Wraps a [`ReleaseManifest`] in a signed envelope so two donto
//! instances can exchange a citable release without re-ingesting
//! the source data. The trust layer is Ed25519 over the manifest's
//! SHA-256, with the public key encoded as `did:key:z…` per the
//! W3C DID spec.
//!
//! Why Ed25519 + did:key (not BBS+ + did:web):
//!   * Ed25519 is in the standard Rust crypto stack — no
//!     pairing-friendly curves needed for the v1 spike.
//!   * did:key encodes the verification key directly in the IRI;
//!     no DNS or HTTP resolution required to verify. That makes
//!     the spike self-contained.
//!
//! The next steps after this spike (see `docs/M9-FEDERATION-MEMO.md`):
//!   * upgrade to BBS+ once selective disclosure is needed
//!     (e.g. for `WITH evidence = redacted_if_required` proofs);
//!   * support did:web for institutional issuers;
//!   * publish to DataCite for citation IDs.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Self-describing key prefix bytes for did:key:Ed25519 per the
/// multicodec table (0xed = ed25519-pub, 0x01 = varint suffix).
const ED25519_MULTICODEC_PREFIX: [u8; 2] = [0xed, 0x01];

#[derive(Debug, Error)]
pub enum EnvelopeError {
    #[error("serialise: {0}")]
    Serialise(#[from] serde_json::Error),
    #[error("malformed did:key: {0}")]
    BadDid(String),
    #[error("signature verification failed: {0}")]
    BadSig(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// An Ed25519 signing keypair.
pub struct Keypair {
    signing: SigningKey,
}

impl Keypair {
    pub fn generate() -> Self {
        let mut rng = OsRng;
        Self {
            signing: SigningKey::generate(&mut rng),
        }
    }
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            signing: SigningKey::from_bytes(&seed),
        }
    }
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }
    pub fn did_key(&self) -> String {
        encode_did_key(&self.verifying_key())
    }
    pub fn seed_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseEnvelope {
    pub manifest_id: String,
    pub manifest_sha256: String,
    pub issuer_did: String,
    pub signature_suite: String,
    pub signature: String,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Sign an arbitrary JSON value (typically a `ReleaseManifest`).
pub fn sign(
    manifest: &serde_json::Value,
    keypair: &Keypair,
) -> Result<ReleaseEnvelope, EnvelopeError> {
    let manifest_id = manifest
        .get("manifest_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let payload = canonical_json(manifest)?;
    let mut hasher = Sha256::new();
    hasher.update(&payload);
    let digest = hasher.finalize();
    let sha_hex = hex::encode(digest);
    let sig: Signature = keypair.signing.sign(&digest);
    Ok(ReleaseEnvelope {
        manifest_id,
        manifest_sha256: sha_hex,
        issuer_did: keypair.did_key(),
        signature_suite: "Ed25519Signature2020".into(),
        signature: bs58::encode(sig.to_bytes()).into_string(),
        created_at: Some(chrono::Utc::now()),
    })
}

/// Verify the envelope. Re-decodes the issuer's did:key into a
/// verifying key and checks the signature against the recorded
/// manifest_sha256.
pub fn verify(env: &ReleaseEnvelope) -> Result<(), EnvelopeError> {
    if env.signature_suite != "Ed25519Signature2020" {
        return Err(EnvelopeError::BadSig(format!(
            "unsupported signature suite `{}`",
            env.signature_suite
        )));
    }
    let vk = decode_did_key(&env.issuer_did)?;
    let sig_bytes = bs58::decode(&env.signature)
        .into_vec()
        .map_err(|e| EnvelopeError::BadSig(format!("bad base58 signature: {e}")))?;
    if sig_bytes.len() != 64 {
        return Err(EnvelopeError::BadSig(format!(
            "expected 64-byte signature, got {}",
            sig_bytes.len()
        )));
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let sig = Signature::from_bytes(&sig_arr);
    let digest = hex::decode(&env.manifest_sha256)
        .map_err(|e| EnvelopeError::BadSig(format!("bad manifest_sha256 hex: {e}")))?;
    vk.verify(&digest, &sig)
        .map_err(|e| EnvelopeError::BadSig(format!("Ed25519 verify failed: {e}")))
}

/// Verify and additionally check that `manifest_sha256` matches a
/// fresh hash of the manifest in hand.
pub fn verify_against_manifest(
    env: &ReleaseEnvelope,
    manifest: &serde_json::Value,
) -> Result<(), EnvelopeError> {
    verify(env)?;
    let payload = canonical_json(manifest)?;
    let mut hasher = Sha256::new();
    hasher.update(&payload);
    let sha_hex = hex::encode(hasher.finalize());
    if sha_hex != env.manifest_sha256 {
        return Err(EnvelopeError::BadSig(format!(
            "manifest hash mismatch: envelope says {} but manifest hashes to {}",
            env.manifest_sha256, sha_hex
        )));
    }
    Ok(())
}

/// JSON Canonicalisation Scheme (RFC 8785) lite: serde_json's
/// sort_keys behaviour plus no insignificant whitespace.
fn canonical_json(v: &serde_json::Value) -> Result<Vec<u8>, EnvelopeError> {
    let mut buf = Vec::new();
    let formatter = serde_json::ser::CompactFormatter;
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    let sorted = sort_value(v);
    serde::Serialize::serialize(&sorted, &mut ser)?;
    Ok(buf)
}

fn sort_value(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut sorted = std::collections::BTreeMap::new();
            for (k, vv) in map {
                sorted.insert(k.clone(), sort_value(vv));
            }
            let mut out = serde_json::Map::new();
            for (k, vv) in sorted {
                out.insert(k, vv);
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(sort_value).collect())
        }
        other => other.clone(),
    }
}

fn encode_did_key(vk: &VerifyingKey) -> String {
    let mut bytes = Vec::with_capacity(2 + 32);
    bytes.extend_from_slice(&ED25519_MULTICODEC_PREFIX);
    bytes.extend_from_slice(vk.as_bytes());
    format!("did:key:z{}", bs58::encode(bytes).into_string())
}

fn decode_did_key(did: &str) -> Result<VerifyingKey, EnvelopeError> {
    let rest = did
        .strip_prefix("did:key:z")
        .ok_or_else(|| EnvelopeError::BadDid(format!("not a did:key:z…: {did}")))?;
    let bytes = bs58::decode(rest)
        .into_vec()
        .map_err(|e| EnvelopeError::BadDid(format!("base58 decode: {e}")))?;
    if bytes.len() != 34 {
        return Err(EnvelopeError::BadDid(format!(
            "expected 34 bytes (2 prefix + 32 key), got {}",
            bytes.len()
        )));
    }
    if bytes[..2] != ED25519_MULTICODEC_PREFIX {
        return Err(EnvelopeError::BadDid(format!(
            "unexpected multicodec prefix 0x{:02x}{:02x}",
            bytes[0], bytes[1]
        )));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes[2..]);
    VerifyingKey::from_bytes(&key)
        .map_err(|e| EnvelopeError::BadDid(format!("verifying key parse: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_sign_and_verify() {
        let manifest = serde_json::json!({
            "manifest_id": "release/wals-toy/2026-05",
            "checksums": [
                {"statement_id": "s2", "sha256": "bb"},
                {"statement_id": "s1", "sha256": "aa"},
            ]
        });
        let kp = Keypair::generate();
        let env = sign(&manifest, &kp).unwrap();
        verify(&env).expect("verify");
        verify_against_manifest(&env, &manifest).expect("verify against manifest");
    }

    #[test]
    fn tampered_manifest_fails_check() {
        let manifest = serde_json::json!({"manifest_id": "x"});
        let kp = Keypair::generate();
        let env = sign(&manifest, &kp).unwrap();
        let tampered = serde_json::json!({"manifest_id": "x", "added_after_sign": true});
        assert!(verify_against_manifest(&env, &tampered).is_err());
    }

    #[test]
    fn key_order_does_not_change_hash() {
        let a = serde_json::json!({"manifest_id": "x", "k": 1, "z": 2});
        let b = serde_json::json!({"z": 2, "manifest_id": "x", "k": 1});
        let kp = Keypair::generate();
        let env_a = sign(&a, &kp).unwrap();
        verify_against_manifest(&env_a, &b).expect("canonical hash matches");
        assert_eq!(env_a.manifest_sha256, sign(&b, &kp).unwrap().manifest_sha256);
    }

    #[test]
    fn different_keys_produce_different_dids() {
        let a = Keypair::generate();
        let b = Keypair::generate();
        assert_ne!(a.did_key(), b.did_key());
    }

    #[test]
    fn seed_round_trip_yields_identical_dids() {
        let seed = [7u8; 32];
        let a = Keypair::from_seed(seed);
        let b = Keypair::from_seed(seed);
        assert_eq!(a.did_key(), b.did_key());
    }

    #[test]
    fn cross_party_verification() {
        // Instance A signs; instance B (which only sees the
        // envelope, not the keypair) verifies against the same
        // manifest. M9 acceptance bullet shape.
        let manifest = serde_json::json!({"manifest_id": "cross-party"});
        let kp_a = Keypair::generate();
        let env = sign(&manifest, &kp_a).unwrap();
        verify_against_manifest(&env, &manifest).expect("B verifies A's envelope");
    }

    #[test]
    fn signature_from_one_keypair_fails_with_another() {
        let manifest = serde_json::json!({"manifest_id": "swap"});
        let kp_a = Keypair::generate();
        let kp_b = Keypair::generate();
        let mut env = sign(&manifest, &kp_a).unwrap();
        env.issuer_did = kp_b.did_key();
        assert!(verify(&env).is_err());
    }

    #[test]
    fn malformed_did_rejected() {
        let manifest = serde_json::json!({"manifest_id": "x"});
        let kp = Keypair::generate();
        let mut env = sign(&manifest, &kp).unwrap();
        env.issuer_did = "did:wrong:abc".into();
        assert!(verify(&env).is_err());
    }
}
