//! One shared rustls client config: native roots, no override for self-signed certificates.

use std::sync::{Arc, OnceLock};

use rustls::{ClientConfig, RootCertStore};

static CONFIG: OnceLock<Result<Arc<ClientConfig>, String>> = OnceLock::new();

/// The process-wide TLS client config, built on first use.
pub(crate) fn client_config() -> Result<Arc<ClientConfig>, String> {
    CONFIG.get_or_init(build).clone()
}

fn build() -> Result<Arc<ClientConfig>, String> {
    let mut roots = RootCertStore::empty();
    let loaded = rustls_native_certs::load_native_certs();
    let (added, _ignored) = roots.add_parsable_certificates(loaded.certs);
    if added == 0 {
        return Err("no usable system root certificates".to_owned());
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}
