// Stub for codex-keyring-store types.

use std::sync::Arc;

/// Re-export the real keyring crate's Error type.
pub use keyring::Error as KeyringError;

/// Stub credential store error.
#[derive(Debug)]
pub enum CredentialStoreError {
    Other(KeyringError),
}

impl CredentialStoreError {
    pub fn new(error: KeyringError) -> Self {
        Self::Other(error)
    }

    pub fn message(&self) -> String {
        match self {
            Self::Other(error) => error.to_string(),
        }
    }

    pub fn into_error(self) -> KeyringError {
        match self {
            Self::Other(error) => error,
        }
    }
}

impl std::fmt::Display for CredentialStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CredentialStoreError {}

/// Stub keyring store trait.
pub trait KeyringStore: Send + Sync {
    fn load(&self, _service: &str, _account: &str) -> Result<Option<String>, CredentialStoreError> {
        Ok(None)
    }
    fn save(&self, _service: &str, _account: &str, _value: &str) -> Result<(), CredentialStoreError> {
        Ok(())
    }
    fn delete(&self, _service: &str, _account: &str) -> Result<bool, CredentialStoreError> {
        Ok(false)
    }
}

/// Stub default keyring store.
pub struct DefaultKeyringStore;

impl DefaultKeyringStore {
    pub fn new() -> Self {
        Self
    }
}

impl KeyringStore for DefaultKeyringStore {}

/// Converts a Box<dyn KeyringStore> into an Arc<dyn KeyringStore>
pub fn into_arc(store: Box<dyn KeyringStore>) -> Arc<dyn KeyringStore> {
    store.into()
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub struct MockKeyringStore;

    impl MockKeyringStore {
        pub fn new() -> Self {
            Self
        }
    }

    impl KeyringStore for MockKeyringStore {
        fn load(&self, _service: &str, _account: &str) -> Result<Option<String>, CredentialStoreError> {
            Ok(None)
        }
        fn save(&self, _service: &str, _account: &str, _value: &str) -> Result<(), CredentialStoreError> {
            Ok(())
        }
        fn delete(&self, _service: &str, _account: &str) -> Result<bool, CredentialStoreError> {
            Ok(false)
        }
    }
}
