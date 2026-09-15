//! Certificate pinning.
//!
//! The phone trusts exactly one certificate: the one whose SHA-256 it learned
//! while pairing. No CA, no host names — a different certificate on the same
//! address is refused, which is what makes plain HTTPS on a home network
//! trustworthy here.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};

use crate::cert::fingerprint_of;

/// Installs the ring provider process-wide (aws-lc needs NASM on Windows).
/// Safe to call repeatedly; only the first call takes effect.
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Debug)]
pub struct PinnedServer {
    expected: String,
    provider: Arc<CryptoProvider>,
}

impl PinnedServer {
    pub fn new(fingerprint: &str) -> Self {
        PinnedServer {
            expected: fingerprint.trim().to_ascii_lowercase(),
            provider: Arc::new(rustls::crypto::ring::default_provider()),
        }
    }
}

impl ServerCertVerifier for PinnedServer {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if constant_time_eq(&fingerprint_of(end_entity), &self.expected) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::General(
                "this isn't the PC you paired with (its certificate changed)".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// A TLS client that accepts only the certificate with this fingerprint.
pub fn pinned_client_config(fingerprint: &str) -> ClientConfig {
    install_crypto_provider();
    ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("ring supports the default protocol versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PinnedServer::new(fingerprint)))
        .with_no_client_auth()
}

/// Compares without leaking where two values differ.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cert::Identity;

    fn verify(pin: &str, cert_der: &[u8]) -> Result<ServerCertVerified, Error> {
        install_crypto_provider();
        PinnedServer::new(pin).verify_server_cert(
            &CertificateDer::from(cert_der.to_vec()),
            &[],
            &ServerName::try_from("dun-pc").unwrap(),
            &[],
            UnixTime::now(),
        )
    }

    #[test]
    fn accepts_only_the_pinned_certificate() {
        let paired = Identity::generate().unwrap();
        let other = Identity::generate().unwrap();

        assert!(verify(&paired.fingerprint(), &paired.cert_der).is_ok());
        assert!(verify(&paired.fingerprint().to_uppercase(), &paired.cert_der).is_ok());
        assert!(
            verify(&paired.fingerprint(), &other.cert_der).is_err(),
            "a different certificate on the same address must be refused"
        );
        assert!(verify("not-a-fingerprint", &paired.cert_der).is_err());
    }

    #[test]
    fn constant_time_eq_still_compares_correctly() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
        assert!(constant_time_eq("", ""));
    }
}
