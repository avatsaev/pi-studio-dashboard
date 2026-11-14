//! The OS keychain behind the engine's `Keychain` hook.
//!
//! Service `com.avatsaev.pistudiodashboard`, account `daemon:<entryId>`, value the UTF-8
//! password. The platform store is reached through the [`Store`] trait so tests use a stub and
//! headless Linux (no Secret Service) is covered by the error mapping: an unavailable backend
//! is an error, never a hang (see `io_worker`, which also bounds every call by a timeout).

use pistudiodashboard_config::APP_IDENTIFIER;
use pistudiodashboard_engine::KeychainError;

/// Account name of a daemon's password.
pub fn account(entry_id: &str) -> String {
    format!("daemon:{entry_id}")
}

/// A credential store: `Ok(None)` when no item exists.
pub trait Store: Send + Sync + 'static {
    fn get(&self, account: &str) -> Result<Option<String>, KeychainError>;
    fn set(&self, account: &str, password: &str) -> Result<(), KeychainError>;
    fn delete(&self, account: &str) -> Result<(), KeychainError>;
}

/// Map a `keyring` error: a missing item is not a failure; an access refusal is `Denied`; every
/// other failure (no backend, no D-Bus session, bad store) means the keychain is unavailable.
pub fn map_error(e: &keyring::Error) -> Result<(), KeychainError> {
    match e {
        keyring::Error::NoEntry => Ok(()),
        keyring::Error::NoStorageAccess(_) => Err(KeychainError::Denied),
        _ => Err(KeychainError::Unavailable),
    }
}

/// The real platform keychain.
pub struct OsStore;

impl OsStore {
    fn entry(account: &str) -> Result<keyring::Entry, KeychainError> {
        keyring::Entry::new(APP_IDENTIFIER, account).map_err(|e| map_error(&e).err().unwrap_or(KeychainError::Unavailable))
    }
}

impl Store for OsStore {
    fn get(&self, account: &str) -> Result<Option<String>, KeychainError> {
        match Self::entry(account)?.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(e) => map_error(&e).map(|()| None),
        }
    }

    fn set(&self, account: &str, password: &str) -> Result<(), KeychainError> {
        Self::entry(account)?.set_password(password).map_err(|e| map_error(&e).err().unwrap_or(KeychainError::Unavailable))
    }

    fn delete(&self, account: &str) -> Result<(), KeychainError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) => Ok(()),
            Err(e) => map_error(&e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_are_namespaced_by_entry() {
        assert_eq!(account("abc"), "daemon:abc");
        assert_eq!(APP_IDENTIFIER, "com.avatsaev.pistudiodashboard");
    }

    #[test]
    fn backend_errors_map_to_the_engine_vocabulary() {
        let boxed = || -> Box<dyn std::error::Error + Send + Sync> { "boom".into() };
        assert_eq!(map_error(&keyring::Error::NoEntry), Ok(()));
        assert_eq!(map_error(&keyring::Error::NoStorageAccess(boxed())), Err(KeychainError::Denied));
        assert_eq!(map_error(&keyring::Error::PlatformFailure(boxed())), Err(KeychainError::Unavailable));
        assert_eq!(map_error(&keyring::Error::NoDefaultStore), Err(KeychainError::Unavailable));
    }

    /// With no Secret Service (headless CI) the real store answers with an error, promptly.
    #[test]
    fn the_real_store_never_hangs() {
        let started = std::time::Instant::now();
        let _ = OsStore.get("daemon:test-never-exists");
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
    }
}
