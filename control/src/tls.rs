use anyhow::{Context, Result};
use rustls::ServerConfig;
use rustls_pemfile::{certs, private_key};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

/// Load TLS server configuration from PEM files.
///
/// If `ca_path` is provided, enables mTLS (client certificate verification).
pub fn load_server_config(
    cert_path: &Path,
    key_path: &Path,
    ca_path: Option<&Path>,
) -> Result<ServerConfig> {
    let cert_chain = load_certs(cert_path)?;
    let key = load_private_key(key_path)?;

    let config = if let Some(ca_path) = ca_path {
        // mTLS: require valid client certificate
        let ca_certs = load_certs(ca_path)?;
        let mut root_store = rustls::RootCertStore::empty();
        for cert in ca_certs {
            root_store
                .add(cert)
                .context("Failed to add CA certificate to root store")?;
        }

        let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
            .build()
            .context("Failed to build client certificate verifier")?;

        ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(cert_chain, key)
            .context("Failed to create mTLS server config")?
    } else {
        // Server TLS only (no client auth)
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)
            .context("Failed to create TLS server config")?
    };

    Ok(config)
}

fn load_certs(path: &Path) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    let file = File::open(path).with_context(|| format!("Failed to open cert file: {:?}", path))?;
    let mut reader = BufReader::new(file);
    let certs: Vec<_> = certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("Failed to parse certificates from {:?}", path))?;

    if certs.is_empty() {
        anyhow::bail!("No certificates found in {:?}", path);
    }

    Ok(certs)
}

fn load_private_key(path: &Path) -> Result<rustls::pki_types::PrivateKeyDer<'static>> {
    let file = File::open(path).with_context(|| format!("Failed to open key file: {:?}", path))?;
    let mut reader = BufReader::new(file);
    let key = private_key(&mut reader)
        .with_context(|| format!("Failed to parse private key from {:?}", path))?
        .with_context(|| format!("No private key found in {:?}", path))?;

    Ok(key)
}
