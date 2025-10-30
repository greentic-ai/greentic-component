use sha2::{Digest as _, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestAlgorithm {
    Sha256,
}

#[derive(Debug, Clone)]
pub struct DigestPolicy {
    algorithm: DigestAlgorithm,
    expected: Option<String>,
    required: bool,
}

impl DigestPolicy {
    pub fn sha256(expected: Option<String>, required: bool) -> Self {
        Self {
            algorithm: DigestAlgorithm::Sha256,
            expected,
            required,
        }
    }

    pub fn expected(&self) -> Option<&str> {
        self.expected.as_deref()
    }

    pub fn verify(&self, bytes: &[u8]) -> Result<VerifiedDigest, VerificationError> {
        let computed = match self.algorithm {
            DigestAlgorithm::Sha256 => {
                let digest = Sha256::digest(bytes);
                VerifiedDigest {
                    algorithm: DigestAlgorithm::Sha256,
                    value: hex::encode(digest),
                }
            }
        };

        if let Some(expected) = &self.expected {
            if !equal_digest(expected, &computed.value) {
                return Err(VerificationError::DigestMismatch {
                    expected: expected.clone(),
                    actual: computed.value,
                });
            }
        } else if self.required {
            return Err(VerificationError::DigestMissing);
        }

        Ok(computed)
    }
}

#[derive(Debug, Clone)]
pub enum SignaturePolicy {
    Disabled,
    Cosign {
        required: bool,
    },
}

impl SignaturePolicy {
    pub fn cosign_required() -> Self {
        SignaturePolicy::Cosign { required: true }
    }

    pub fn cosign_optional() -> Self {
        SignaturePolicy::Cosign { required: false }
    }

    pub fn verify(&self, _bytes: &[u8]) -> Result<VerifiedSignature, VerificationError> {
        match self {
            SignaturePolicy::Disabled => Ok(VerifiedSignature::Skipped),
            SignaturePolicy::Cosign { required } => {
                if *required {
                    Err(VerificationError::SignatureNotImplemented(
                        "cosign signature verification required".into(),
                    ))
                } else {
                    Ok(VerifiedSignature::Skipped)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct VerificationPolicy {
    pub digest: Option<DigestPolicy>,
    pub signature: Option<SignaturePolicy>,
}

impl VerificationPolicy {
    pub fn verify(&self, bytes: &[u8]) -> Result<VerificationReport, VerificationError> {
        let digest = match &self.digest {
            Some(policy) => Some(policy.verify(bytes)?),
            None => None,
        };
        let signature = match &self.signature {
            Some(policy) => Some(policy.verify(bytes)?),
            None => None,
        };
        Ok(VerificationReport { digest, signature })
    }
}

#[derive(Debug, Clone)]
pub struct VerificationReport {
    pub digest: Option<VerifiedDigest>,
    pub signature: Option<VerifiedSignature>,
}

#[derive(Debug, Clone)]
pub struct VerifiedDigest {
    pub algorithm: DigestAlgorithm,
    pub value: String,
}

impl VerifiedDigest {
    pub fn compute(algorithm: DigestAlgorithm, bytes: &[u8]) -> Self {
        match algorithm {
            DigestAlgorithm::Sha256 => {
                let digest = Sha256::digest(bytes);
                Self {
                    algorithm,
                    value: hex::encode(digest),
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum VerifiedSignature {
    Skipped,
}

#[derive(Debug, Error)]
pub enum VerificationError {
    #[error("digest check required but no expected value provided")]
    DigestMissing,
    #[error("digest mismatch (expected {expected}, actual {actual})")]
    DigestMismatch { expected: String, actual: String },
    #[error("signature verification not implemented: {0}")]
    SignatureNotImplemented(String),
}

fn equal_digest(expected: &str, actual: &str) -> bool {
    expected.eq_ignore_ascii_case(actual)
}
