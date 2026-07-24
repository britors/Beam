//! TOFU (trust-on-first-use) certificate fingerprint store.
//!
//! Mirrors the SSH `known_hosts` model: the SHA-256 fingerprint of the leaf certificate
//! presented on the first successful connection to `host:port` is remembered. On later
//! connections a mismatch is treated as a potential man-in-the-middle attempt and must be
//! explicitly confirmed by the user before continuing.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::profile::{config_dir, ProfileError};

/// SHA-256 fingerprint of a DER-encoded certificate, formatted as lowercase colon-separated hex
/// (e.g. `af:01:...`), matching the conventional display format for certificate fingerprints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint(String);

impl Fingerprint {
    pub fn of_der(der: &[u8]) -> Self {
        let digest = Sha256::digest(der);
        let hex: Vec<String> = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        Self(hex.join(":"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct KnownHostsStore {
    #[serde(default, flatten)]
    hosts: BTreeMap<String, Fingerprint>,
}

fn known_hosts_path() -> Result<PathBuf, ProfileError> {
    Ok(config_dir()?.join("known_hosts.toml"))
}

fn load_store() -> Result<KnownHostsStore, ProfileError> {
    let path = known_hosts_path()?;
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(KnownHostsStore::default()),
        Err(source) => return Err(ProfileError::Io { path, source }),
    };
    Ok(toml::from_str(&contents)?)
}

fn save_store(store: &KnownHostsStore) -> Result<(), ProfileError> {
    let path = known_hosts_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ProfileError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let contents = toml::to_string_pretty(store)?;
    fs::write(&path, contents).map_err(|source| ProfileError::Io { path, source })
}

/// Result of checking a certificate fingerprint against the known-hosts store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustDecision {
    /// This is the first time we connect to this `host:port`; the user must confirm.
    FirstUse,
    /// The fingerprint matches the one recorded on a previous successful connection.
    Trusted,
    /// The fingerprint differs from the one recorded previously — possible MITM. The user must
    /// explicitly confirm before we proceed.
    Mismatch { previous: Fingerprint },
}

/// Look up `address` (`host:port`) in the known-hosts store and compare against `fingerprint`.
pub fn check(address: &str, fingerprint: &Fingerprint) -> Result<TrustDecision, ProfileError> {
    let store = load_store()?;
    match store.hosts.get(address) {
        None => Ok(TrustDecision::FirstUse),
        Some(previous) if previous == fingerprint => Ok(TrustDecision::Trusted),
        Some(previous) => Ok(TrustDecision::Mismatch {
            previous: previous.clone(),
        }),
    }
}

/// Record `fingerprint` as trusted for `address`, overwriting any previous entry.
pub fn trust(address: &str, fingerprint: &Fingerprint) -> Result<(), ProfileError> {
    let mut store = load_store()?;
    store.hosts.insert(address.to_owned(), fingerprint.clone());
    save_store(&store)
}

/// List every trusted `(address, fingerprint)` entry, sorted by address.
pub fn list() -> Result<Vec<(String, Fingerprint)>, ProfileError> {
    let store = load_store()?;
    Ok(store.hosts.into_iter().collect())
}

/// Remove `address` from the known-hosts store, if present; the next connection to it will
/// require first-use confirmation again.
pub fn remove(address: &str) -> Result<(), ProfileError> {
    let mut store = load_store()?;
    store.hosts.remove(address);
    save_store(&store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_lowercase_colon_separated_sha256() {
        let fp = Fingerprint::of_der(b"hello world");
        // SHA-256("hello world") = b94d27b9934d3e08a52e52d7da7dacefbd...
        assert!(fp.as_str().starts_with("b9:4d:27:b9"));
        assert_eq!(fp.as_str().len(), 32 * 3 - 1); // 32 bytes -> "xx:" * 31 + "xx"
        assert!(fp.as_str().chars().all(|c| c.is_ascii_hexdigit() || c == ':'));
    }

    #[test]
    fn fingerprint_is_deterministic_and_input_sensitive() {
        let a = Fingerprint::of_der(b"certificate A");
        let b = Fingerprint::of_der(b"certificate A");
        let c = Fingerprint::of_der(b"certificate B");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn store_round_trips_through_toml() {
        let mut store = KnownHostsStore::default();
        store.hosts.insert(
            "10.0.0.5:3389".to_owned(),
            Fingerprint::of_der(b"cert bytes"),
        );
        let text = toml::to_string_pretty(&store).expect("serialize");
        let parsed: KnownHostsStore = toml::from_str(&text).expect("deserialize");
        assert_eq!(parsed.hosts.get("10.0.0.5:3389"), store.hosts.get("10.0.0.5:3389"));
    }
}
