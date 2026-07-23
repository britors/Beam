//! Connection profiles: persisted (password-less) connection settings.

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn default_port() -> u16 {
    3389
}

fn default_color_depth() -> u32 {
    32
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u16,
    pub height: u16,
}

impl Resolution {
    pub const PRESETS: &'static [Resolution] = &[
        Resolution { width: 1280, height: 720 },
        Resolution { width: 1366, height: 768 },
        Resolution { width: 1600, height: 900 },
        Resolution { width: 1920, height: 1080 },
        Resolution { width: 2560, height: 1440 },
    ];
}

impl Default for Resolution {
    fn default() -> Self {
        Resolution { width: 1366, height: 768 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProfile {
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub resolution: Resolution,
    #[serde(default = "default_color_depth")]
    pub color_depth: u32,
    #[serde(default = "default_true")]
    pub fullscreen: bool,
}

impl ConnectionProfile {
    pub fn new(name: impl Into<String>, host: impl Into<String>, username: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            host: host.into(),
            port: default_port(),
            username: username.into(),
            domain: None,
            resolution: Resolution::default(),
            color_depth: default_color_depth(),
            fullscreen: true,
        }
    }

    /// `host:port`, used as the network destination and as the `known_hosts` key.
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn duplicate(&self) -> Self {
        let mut copy = self.clone();
        copy.id = Uuid::new_v4();
        copy.name = format!("{} (cópia)", self.name);
        copy
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ProfileStore {
    #[serde(default, rename = "profile")]
    profiles: Vec<ConnectionProfile>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("falha de E/S ao acessar {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("arquivo de configuração inválido: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("falha ao serializar configuração: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("não foi possível determinar o diretório de configuração do usuário")]
    NoConfigDir,
}

pub fn config_dir() -> Result<PathBuf, ProfileError> {
    let dirs = directories::ProjectDirs::from("org", "lyraos", "beam").ok_or(ProfileError::NoConfigDir)?;
    Ok(dirs.config_dir().to_path_buf())
}

fn connections_path() -> Result<PathBuf, ProfileError> {
    Ok(config_dir()?.join("connections.toml"))
}

/// Load every saved connection profile from `~/.config/beam/connections.toml`.
///
/// A missing file is treated as "no profiles yet", not an error.
pub fn load_profiles() -> Result<Vec<ConnectionProfile>, ProfileError> {
    let path = connections_path()?;
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(ProfileError::Io { path, source }),
    };
    let store: ProfileStore = toml::from_str(&contents)?;
    Ok(store.profiles)
}

/// Persist the full set of profiles, replacing the file's previous contents.
pub fn save_profiles(profiles: &[ConnectionProfile]) -> Result<(), ProfileError> {
    let path = connections_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ProfileError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let store = ProfileStore {
        profiles: profiles.to_vec(),
    };
    let contents = toml::to_string_pretty(&store)?;
    fs::write(&path, contents).map_err(|source| ProfileError::Io { path, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let p = ConnectionProfile::new("Servidor", "10.0.0.5", "admin");
        assert_eq!(p.port, 3389);
        assert_eq!(p.color_depth, 32);
        assert!(p.fullscreen);
        assert_eq!(p.domain, None);
        assert_eq!(p.resolution, Resolution { width: 1366, height: 768 });
    }

    #[test]
    fn address_formats_host_and_port() {
        let mut p = ConnectionProfile::new("S", "win.example.com", "u");
        p.port = 3390;
        assert_eq!(p.address(), "win.example.com:3390");
    }

    #[test]
    fn duplicate_gets_new_id_and_suffixed_name() {
        let p = ConnectionProfile::new("Servidor", "10.0.0.5", "admin");
        let d = p.duplicate();
        assert_ne!(p.id, d.id);
        assert_eq!(d.name, "Servidor (cópia)");
        assert_eq!(d.host, p.host);
    }

    #[test]
    fn profile_store_round_trips_through_toml() {
        let mut a = ConnectionProfile::new("A", "a.example.com", "alice");
        a.domain = Some("CORP".to_owned());
        a.resolution = Resolution { width: 1920, height: 1080 };
        let b = ConnectionProfile::new("B", "b.example.com", "bob");

        let store = ProfileStore {
            profiles: vec![a.clone(), b.clone()],
        };
        let toml_text = toml::to_string_pretty(&store).expect("serialize");
        let parsed: ProfileStore = toml::from_str(&toml_text).expect("deserialize");

        assert_eq!(parsed.profiles.len(), 2);
        assert_eq!(parsed.profiles[0].name, a.name);
        assert_eq!(parsed.profiles[0].domain, a.domain);
        assert_eq!(parsed.profiles[0].resolution, a.resolution);
        assert_eq!(parsed.profiles[1].name, b.name);

        // Passwords must never appear anywhere in the serialized form.
        assert!(!toml_text.to_lowercase().contains("senha"));
        assert!(!toml_text.to_lowercase().contains("password"));
    }

    #[test]
    fn missing_fields_fall_back_to_defaults_for_forward_compatibility() {
        // A minimal, older-shaped document (as if a future version added fields we don't know
        // about, or this file predates a field being introduced) must still parse.
        let minimal = r#"
            [[profile]]
            name = "Legado"
            host = "legacy.example.com"
            username = "user"
        "#;
        let store: ProfileStore = toml::from_str(minimal).expect("deserialize minimal profile");
        assert_eq!(store.profiles.len(), 1);
        assert_eq!(store.profiles[0].port, 3389);
        assert_eq!(store.profiles[0].color_depth, 32);
    }
}
