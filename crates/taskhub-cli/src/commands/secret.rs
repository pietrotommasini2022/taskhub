use anyhow::{Context, Result};
use taskhub_core::{credentials::CredentialStore, Storage};

use crate::commands::init::db_path;

fn storage() -> Result<Storage> {
    let db = db_path()?;
    if !db.exists() {
        anyhow::bail!("taskhub not initialized — run `taskhub init` first");
    }
    Storage::open(&db).context("open storage")
}

pub fn set(key: &str) -> Result<()> {
    let value = rpassword::prompt_password(format!("Value for {key}: "))
        .context("read secret")?;
    if value.is_empty() {
        anyhow::bail!("secret value cannot be empty");
    }
    let s = storage()?;
    CredentialStore::new(&s).set(key, &value)?;
    println!("Secret '{key}' stored.");
    Ok(())
}

pub fn list() -> Result<()> {
    let s = storage()?;
    let keys = CredentialStore::new(&s).list()?;
    if keys.is_empty() {
        println!("No secrets stored.");
    } else {
        for k in &keys {
            println!("  {k}");
        }
    }
    Ok(())
}

pub fn remove(key: &str) -> Result<()> {
    let s = storage()?;
    CredentialStore::new(&s).remove(key)?;
    println!("Secret '{key}' removed.");
    Ok(())
}
