//! Coinbase Developer Platform (CDP) JWT bearer-token signer.
//!
//! CDP's x402 facilitator at `https://api.cdp.coinbase.com/platform/v2/x402`
//! requires every request to carry an `Authorization: Bearer <JWT>` header.
//! The JWT is short-lived (2 minutes), Ed25519-signed (EdDSA), and bound to
//! the request URI, so it must be regenerated per request.
//!
//! This module exposes [`CdpJwtSigner`], which is constructed once from a CDP
//! API key ID (UUID) and its 64-byte Ed25519 secret (standard-base64-encoded,
//! `seed || public_key`), and produces a signed JWT for any `(method, host,
//! path)` triple.
//!
//! SDK users can attach a signer to a [`crate::payment::facilitator_client::FacilitatorClient`]
//! via [`crate::payment::facilitator_client::FacilitatorClient::with_cdp_signer`].

use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer, SigningKey};
use serde::Serialize;

/// Signs CDP JWT bearer tokens. Cheap to clone; holds a 32-byte signing key.
#[derive(Clone)]
pub struct CdpJwtSigner {
    key_id: String,
    signing_key: SigningKey,
}

impl std::fmt::Debug for CdpJwtSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CdpJwtSigner")
            .field("key_id", &self.key_id)
            .field("signing_key", &"<redacted>")
            .finish()
    }
}

/// Errors constructing or using a [`CdpJwtSigner`].
#[derive(Debug, thiserror::Error)]
pub enum CdpJwtError {
    #[error("CDP key_id must not be empty")]
    EmptyKeyId,
    #[error("CDP key_secret must not be empty")]
    EmptyKeySecret,
    #[error("CDP key_secret is not valid base64: {0}")]
    SecretBase64(#[source] base64::DecodeError),
    #[error("CDP key_secret must decode to 64 bytes (seed || public_key); got {0} bytes")]
    SecretLength(usize),
    #[error("Failed to serialize JWT {part}: {source}")]
    JsonSerialize {
        part: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("System time is before the UNIX epoch")]
    SystemClockBeforeEpoch,
    #[error("Failed to generate JWT nonce: {0}")]
    NonceRng(#[source] getrandom::Error),
}

#[derive(Serialize)]
struct Header<'a> {
    alg: &'static str,
    kid: &'a str,
    typ: &'static str,
    nonce: String,
}

#[derive(Serialize)]
struct Claims<'a> {
    sub: &'a str,
    iss: &'static str,
    nbf: u64,
    exp: u64,
    uris: [String; 1],
}

impl CdpJwtSigner {
    /// Builds a signer from a CDP API key ID and its base64-encoded 64-byte
    /// Ed25519 secret (seed || public_key, as issued by the CDP portal).
    pub fn try_new(key_id: impl Into<String>, key_secret_b64: &str) -> Result<Self, CdpJwtError> {
        let key_id = key_id.into();
        if key_id.is_empty() {
            return Err(CdpJwtError::EmptyKeyId);
        }
        if key_secret_b64.is_empty() {
            return Err(CdpJwtError::EmptyKeySecret);
        }

        let decoded = STANDARD
            .decode(key_secret_b64.trim())
            .map_err(CdpJwtError::SecretBase64)?;
        if decoded.len() != 64 {
            return Err(CdpJwtError::SecretLength(decoded.len()));
        }

        let mut seed = [0u8; 32];
        seed.copy_from_slice(&decoded[..32]);
        let signing_key = SigningKey::from_bytes(&seed);

        Ok(Self {
            key_id,
            signing_key,
        })
    }

    /// Returns the configured key ID. Useful for diagnostics.
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// Produces a freshly signed JWT bound to the given request URI.
    ///
    /// `method` should be uppercase HTTP method (`GET`, `POST`); `host` is the
    /// bare host (no scheme, no port unless non-default); `path_and_query` is
    /// the request path with optional query string.
    pub fn sign(
        &self,
        method: &str,
        host: &str,
        path_and_query: &str,
    ) -> Result<String, CdpJwtError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| CdpJwtError::SystemClockBeforeEpoch)?
            .as_secs();

        let mut nonce_bytes = [0u8; 16];
        getrandom::fill(&mut nonce_bytes).map_err(CdpJwtError::NonceRng)?;
        let nonce = hex_encode(&nonce_bytes);

        let header = Header {
            alg: "EdDSA",
            kid: &self.key_id,
            typ: "JWT",
            nonce,
        };
        let claims = Claims {
            sub: &self.key_id,
            iss: "cdp",
            nbf: now,
            exp: now + 120,
            uris: [format!("{method} {host}{path_and_query}")],
        };

        let header_json = serde_json::to_vec(&header).map_err(|e| CdpJwtError::JsonSerialize {
            part: "header",
            source: e,
        })?;
        let claims_json = serde_json::to_vec(&claims).map_err(|e| CdpJwtError::JsonSerialize {
            part: "claims",
            source: e,
        })?;

        let header_b64 = URL_SAFE_NO_PAD.encode(&header_json);
        let claims_b64 = URL_SAFE_NO_PAD.encode(&claims_json);

        let signing_input = format!("{header_b64}.{claims_b64}");
        let signature = self.signing_key.sign(signing_input.as_bytes());
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        Ok(format!("{signing_input}.{sig_b64}"))
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(HEX[(b >> 4) as usize]));
        out.push(char::from(HEX[(b & 0x0f) as usize]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Result, anyhow};

    fn make_signer() -> Result<CdpJwtSigner> {
        let seed = [7u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let mut full = [0u8; 64];
        full[..32].copy_from_slice(&seed);
        full[32..].copy_from_slice(sk.verifying_key().as_bytes());
        let b64 = STANDARD.encode(full);
        CdpJwtSigner::try_new("test-key-id", &b64).map_err(|e| anyhow!("{e}"))
    }

    #[test]
    fn rejects_empty_key_id() -> Result<()> {
        match CdpJwtSigner::try_new("", "AA==") {
            Err(CdpJwtError::EmptyKeyId) => Ok(()),
            other => Err(anyhow!("expected EmptyKeyId, got {other:?}")),
        }
    }

    #[test]
    fn rejects_wrong_secret_length() -> Result<()> {
        let bad = STANDARD.encode([1u8; 32]);
        match CdpJwtSigner::try_new("kid", &bad) {
            Err(CdpJwtError::SecretLength(32)) => Ok(()),
            other => Err(anyhow!("expected SecretLength(32), got {other:?}")),
        }
    }

    #[test]
    fn signs_three_segment_jwt() -> Result<()> {
        let signer = make_signer()?;
        let jwt = signer
            .sign("POST", "api.cdp.coinbase.com", "/platform/v2/x402/verify")
            .map_err(|e| anyhow!("{e}"))?;
        let parts: Vec<&str> = jwt.split('.').collect();
        if parts.len() != 3 {
            return Err(anyhow!("expected 3 JWT segments, got {}", parts.len()));
        }
        for p in &parts {
            if p.is_empty() {
                return Err(anyhow!("empty JWT segment"));
            }
            URL_SAFE_NO_PAD
                .decode(p)
                .map_err(|e| anyhow!("segment not URL-safe base64: {e}"))?;
        }
        Ok(())
    }

    #[test]
    fn signature_verifies_against_public_key() -> Result<()> {
        let signer = make_signer()?;
        let jwt = signer
            .sign("GET", "api.cdp.coinbase.com", "/x")
            .map_err(|e| anyhow!("{e}"))?;
        let parts: Vec<&str> = jwt.split('.').collect();
        if parts.len() != 3 {
            return Err(anyhow!("expected 3 JWT segments"));
        }
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let sig_bytes = URL_SAFE_NO_PAD
            .decode(parts[2])
            .map_err(|e| anyhow!("decode sig: {e}"))?;
        let sig_arr: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| anyhow!("signature is not 64 bytes"))?;
        let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
        signer
            .signing_key
            .verifying_key()
            .verify_strict(signing_input.as_bytes(), &sig)
            .map_err(|e| anyhow!("signature did not verify: {e}"))?;
        Ok(())
    }
}
