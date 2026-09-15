//! The PC's self-signed certificate.
//!
//! There is no CA anywhere: the phone pins this certificate's SHA-256 when it
//! pairs (the fingerprint travels in the QR code), so the identity is the key
//! itself. The pair is generated once and kept in the app data `sync/` folder.

use std::path::{Path, PathBuf};

use rcgen::{CertificateParams, KeyPair};
use sha2::{Digest, Sha256};

const CERT_FILE: &str = "server-cert.pem";
const KEY_FILE: &str = "server-key.pem";
/// Long enough that it never expires in practice; pinning is what matters.
const VALID_FROM: i32 = 2020;
const YEARS: i32 = 40;

#[derive(Debug, thiserror::Error)]
pub enum CertError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("couldn't create Dun's certificate: {0}")]
    Generate(#[from] rcgen::Error),
    #[error("the stored certificate is unreadable; delete {0} to make a new one")]
    Unreadable(PathBuf),
}

/// A certificate and its key, plus the fingerprint the phone pins.
#[derive(Clone)]
pub struct Identity {
    pub cert_pem: String,
    pub key_pem: String,
    pub cert_der: Vec<u8>,
}

impl Identity {
    /// Loads the stored pair, generating one the first time.
    pub fn load_or_create(dir: &Path) -> Result<Identity, CertError> {
        let (cert_path, key_path) = (dir.join(CERT_FILE), dir.join(KEY_FILE));
        if cert_path.exists() && key_path.exists() {
            let cert_pem = read(&cert_path)?;
            let key_pem = read(&key_path)?;
            let der =
                pem_to_der(&cert_pem).ok_or_else(|| CertError::Unreadable(cert_path.clone()))?;
            return Ok(Identity {
                cert_pem,
                key_pem,
                cert_der: der,
            });
        }

        let identity = Identity::generate()?;
        std::fs::create_dir_all(dir).map_err(|source| CertError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        write(&cert_path, &identity.cert_pem)?;
        write(&key_path, &identity.key_pem)?;
        Ok(identity)
    }

    pub fn generate() -> Result<Identity, CertError> {
        let mut params = CertificateParams::new(vec!["dun-pc".to_string()])?;
        // A fixed, wide window: identity comes from the pinned fingerprint, and
        // a fixed window means the certificate can't "expire" mid-use or depend
        // on the clock when it was made.
        params.not_before = rcgen::date_time_ymd(VALID_FROM, 1, 1);
        params.not_after = rcgen::date_time_ymd(VALID_FROM + YEARS, 1, 1);
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "Dun");

        let signing_key = KeyPair::generate()?;
        let cert = params.self_signed(&signing_key)?;
        Ok(Identity {
            cert_pem: cert.pem(),
            key_pem: signing_key.serialize_pem(),
            cert_der: cert.der().to_vec(),
        })
    }

    /// Lower-case hex SHA-256 of the certificate, as pinned by the phone.
    pub fn fingerprint(&self) -> String {
        fingerprint_of(&self.cert_der)
    }
}

pub fn fingerprint_of(cert_der: &[u8]) -> String {
    let digest = Sha256::digest(cert_der);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn read(path: &Path) -> Result<String, CertError> {
    std::fs::read_to_string(path).map_err(|source| CertError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write(path: &Path, contents: &str) -> Result<(), CertError> {
    std::fs::write(path, contents).map_err(|source| CertError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Minimal PEM decode for the one certificate we store.
fn pem_to_der(pem: &str) -> Option<Vec<u8>> {
    let body: String = pem
        .lines()
        .skip_while(|l| !l.starts_with("-----BEGIN CERTIFICATE"))
        .skip(1)
        .take_while(|l| !l.starts_with("-----END CERTIFICATE"))
        .collect();
    base64_decode(body.trim())
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes().filter(|b| !b.is_ascii_whitespace()) {
        if byte == b'=' {
            break;
        }
        let value = TABLE.iter().position(|c| *c == byte)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_once_and_reloads_the_same_certificate() {
        let dir = tempfile::tempdir().unwrap();
        let first = Identity::load_or_create(dir.path()).unwrap();
        let again = Identity::load_or_create(dir.path()).unwrap();
        assert_eq!(first.cert_pem, again.cert_pem);
        assert_eq!(first.fingerprint(), again.fingerprint());
        assert_eq!(first.fingerprint().len(), 64);
        assert!(first.fingerprint().chars().all(|c| c.is_ascii_hexdigit()));
        assert!(dir.path().join(CERT_FILE).exists());
    }

    #[test]
    fn different_installs_get_different_identities() {
        let a = Identity::generate().unwrap();
        let b = Identity::generate().unwrap();
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn pem_round_trips_to_the_same_der() {
        let identity = Identity::generate().unwrap();
        assert_eq!(pem_to_der(&identity.cert_pem).unwrap(), identity.cert_der);
    }
}
