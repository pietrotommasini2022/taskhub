use crate::error::TaskHubError;
use crate::storage::Storage;
use age::secrecy::Secret;
use anyhow::Result;
use std::io::{Read, Write};
use tracing::debug;

const KEYRING_SERVICE: &str = "taskhub";
const KEYRING_USER: &str = "master-key";

pub struct CredentialStore<'a> {
    storage: &'a Storage,
}

impl<'a> CredentialStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    pub fn set(&self, key: &str, value: &str) -> Result<(), TaskHubError> {
        let master = self.get_or_create_master_key()?;
        let ciphertext = encrypt(value.as_bytes(), &master)
            .map_err(|e| TaskHubError::Plugin(format!("encrypt: {e}")))?;
        self.storage.upsert_credential(key, &ciphertext)?;
        debug!(key, "credential stored");
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<String, TaskHubError> {
        let ciphertext = self.storage.get_credential(key)?
            .ok_or_else(|| TaskHubError::SecretNotFound(key.to_string()))?;
        let master = self.get_or_create_master_key()?;
        let plaintext = decrypt(&ciphertext, &master)
            .map_err(|e| TaskHubError::Plugin(format!("decrypt '{key}': {e}")))?;
        String::from_utf8(plaintext).map_err(|e| TaskHubError::Plugin(e.to_string()))
    }

    pub fn list(&self) -> Result<Vec<String>, TaskHubError> {
        self.storage.list_credential_keys()
    }

    pub fn remove(&self, key: &str) -> Result<(), TaskHubError> {
        self.storage.delete_credential(key)
    }

    fn get_or_create_master_key(&self) -> Result<String, TaskHubError> {
        // Try OS keychain first.
        match get_from_keychain() {
            Ok(k) => return Ok(k),
            Err(e) => debug!("keychain unavailable ({}), using file fallback", e),
        }
        // File fallback: ~/.taskhub/master.key
        let path = dirs::home_dir()
            .ok_or_else(|| TaskHubError::Plugin("no home dir".into()))?
            .join(".taskhub")
            .join("master.key");
        if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .map_err(|e| TaskHubError::Plugin(e.to_string()))?;
            return Ok(raw.trim().to_string());
        }
        // Generate new master key.
        let key = generate_key();
        if let Err(e) = set_in_keychain(&key) {
            debug!("keychain write failed ({}), saving to file", e);
            std::fs::create_dir_all(path.parent().unwrap()).ok();
            std::fs::write(&path, &key)
                .map_err(|e| TaskHubError::Plugin(format!("write master key: {e}")))?;
        }
        Ok(key)
    }
}

fn generate_key() -> String {
    // 64-char hex key from two UUID v7s.
    let a = uuid::Uuid::now_v7().simple().to_string();
    let b = uuid::Uuid::now_v7().simple().to_string();
    format!("{a}{b}")
}

fn get_from_keychain() -> Result<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
    Ok(entry.get_password()?)
}

fn set_in_keychain(key: &str) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
    entry.set_password(key)?;
    Ok(())
}

fn encrypt(plaintext: &[u8], passphrase: &str) -> Result<Vec<u8>> {
    let encryptor = age::Encryptor::with_user_passphrase(Secret::new(passphrase.to_string()));
    let mut ciphertext = vec![];
    let mut writer = encryptor.wrap_output(&mut ciphertext)?;
    writer.write_all(plaintext)?;
    writer.finish()?;
    Ok(ciphertext)
}

fn decrypt(ciphertext: &[u8], passphrase: &str) -> Result<Vec<u8>> {
    let decryptor = match age::Decryptor::new(ciphertext)? {
        age::Decryptor::Passphrase(d) => d,
        _ => anyhow::bail!("unexpected age decryptor type"),
    };
    let mut plaintext = vec![];
    let mut reader = decryptor.decrypt(&Secret::new(passphrase.to_string()), None)?;
    reader.read_to_end(&mut plaintext)?;
    Ok(plaintext)
}
