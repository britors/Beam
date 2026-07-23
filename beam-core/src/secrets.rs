//! Password storage via the freedesktop Secret Service (GNOME Keyring / KWallet), through the
//! pure-Rust `oo7` client. Passwords are never written to `connections.toml` or to logs.

use std::collections::HashMap;

use oo7::{AsAttributes, Secret};

const APP_ATTRIBUTE: &str = "beam";

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("falha ao acessar o chaveiro do sistema: {0}")]
    Backend(#[from] oo7::Error),
}

/// Identifies a stored credential: one password per (host, port, user) triple.
#[derive(Debug, Clone)]
pub struct SecretKey<'a> {
    pub host: &'a str,
    pub port: u16,
    pub user: &'a str,
}

impl SecretKey<'_> {
    fn attributes(&self) -> HashMap<String, String> {
        let port = self.port.to_string();
        vec![
            ("app", APP_ATTRIBUTE),
            ("host", self.host),
            ("port", port.as_str()),
            ("user", self.user),
        ]
        .as_attributes()
    }

    fn label(&self) -> String {
        format!("Beam — {}@{}:{}", self.user, self.host, self.port)
    }
}

/// Store (or replace) the password for `key` in the Secret Service.
pub async fn store_password(key: &SecretKey<'_>, password: &str) -> Result<(), SecretError> {
    let keyring = oo7::Keyring::new().await?;
    keyring
        .create_item(&key.label(), &key.attributes(), Secret::text(password), true)
        .await?;
    Ok(())
}

/// Look up the password for `key`, if one was previously stored.
pub async fn lookup_password(key: &SecretKey<'_>) -> Result<Option<String>, SecretError> {
    let keyring = oo7::Keyring::new().await?;
    let items = keyring.search_items(&key.attributes()).await?;
    let Some(item) = items.into_iter().next() else {
        return Ok(None);
    };
    let secret = item.secret().await?;
    Ok(Some(String::from_utf8_lossy(secret.as_bytes()).into_owned()))
}

/// Remove the stored password for `key`, if any. Used when a profile is deleted.
pub async fn delete_password(key: &SecretKey<'_>) -> Result<(), SecretError> {
    let keyring = oo7::Keyring::new().await?;
    keyring.delete(&key.attributes()).await?;
    Ok(())
}
