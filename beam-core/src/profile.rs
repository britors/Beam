//! Connection profiles: persisted (password-less) connection settings.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

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
        Resolution {
            width: 1280,
            height: 720,
        },
        Resolution {
            width: 1366,
            height: 768,
        },
        Resolution {
            width: 1600,
            height: 900,
        },
        Resolution {
            width: 1920,
            height: 1080,
        },
        Resolution {
            width: 2560,
            height: 1440,
        },
    ];
}

impl Default for Resolution {
    fn default() -> Self {
        Resolution {
            width: 1366,
            height: 768,
        }
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
    pub fn new(
        name: impl Into<String>,
        host: impl Into<String>,
        username: impl Into<String>,
    ) -> Self {
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

    /// Hostname without the optional brackets accepted around an IPv6 literal.
    pub fn normalized_host(&self) -> &str {
        self.host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(&self.host)
    }

    /// Canonical `host:port`, suitable for socket resolution and the `known_hosts` key.
    pub fn address(&self) -> String {
        let host = self.normalized_host();
        if host.contains(':') {
            format!("[{host}]:{}", self.port)
        } else {
            format!("{host}:{}", self.port)
        }
    }

    pub fn duplicate(&self, copy_suffix: &str) -> Self {
        let mut copy = self.clone();
        copy.id = Uuid::new_v4();
        copy.name = format!("{} ({copy_suffix})", self.name);
        copy
    }
}

/// Durably replace `path` without ever exposing a partially-written destination.
pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    atomic_write_impl(path, contents, true)
}

fn atomic_write_impl(path: &Path, contents: &[u8], preserve_old: bool) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("beam");
    let temp = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        if preserve_old && path.exists() {
            let backup = backup_path(path);
            fs::copy(path, &backup)?;
            fs::File::open(&backup)?.sync_all()?;
        }
        fs::rename(&temp, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub(crate) fn restore_backup(path: &Path, contents: &[u8]) -> io::Result<()> {
    atomic_write_impl(path, contents, false)
}

pub(crate) fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("toml.bak")
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
    let dirs =
        directories::ProjectDirs::from("org", "lyraos", "beam").ok_or(ProfileError::NoConfigDir)?;
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
    let store: ProfileStore = match toml::from_str(&contents) {
        Ok(store) => store,
        Err(primary_error) => match fs::read_to_string(backup_path(&path)) {
            Ok(backup) => {
                let store = toml::from_str(&backup).map_err(|_| primary_error)?;
                restore_backup(&path, backup.as_bytes()).map_err(|source| ProfileError::Io {
                    path: path.clone(),
                    source,
                })?;
                store
            }
            Err(_) => return Err(primary_error.into()),
        },
    };
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
    atomic_write(&path, contents.as_bytes()).map_err(|source| ProfileError::Io { path, source })
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
        assert_eq!(
            p.resolution,
            Resolution {
                width: 1366,
                height: 768
            }
        );
    }

    #[test]
    fn address_formats_host_and_port() {
        let mut p = ConnectionProfile::new("S", "win.example.com", "u");
        p.port = 3390;
        assert_eq!(p.address(), "win.example.com:3390");
    }

    #[test]
    fn address_canonicalizes_ipv6_literals() {
        for host in ["2001:db8::1", "[2001:db8::1]", "::1", "fe80::1%eth0"] {
            let p = ConnectionProfile::new("S", host, "u");
            assert_eq!(p.address(), format!("[{}]:3389", p.normalized_host()));
        }
    }

    #[test]
    fn duplicate_gets_new_id_and_suffixed_name() {
        let p = ConnectionProfile::new("Servidor", "10.0.0.5", "admin");
        let d = p.duplicate("copy");
        assert_ne!(p.id, d.id);
        assert_eq!(d.name, "Servidor (copy)");
        assert_eq!(d.host, p.host);
    }

    #[test]
    fn profile_store_round_trips_through_toml() {
        let mut a = ConnectionProfile::new("A", "a.example.com", "alice");
        a.domain = Some("CORP".to_owned());
        a.resolution = Resolution {
            width: 1920,
            height: 1080,
        };
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

    #[test]
    fn atomic_write_keeps_last_good_backup() {
        let directory = std::env::temp_dir().join(format!("beam-atomic-{}", Uuid::new_v4()));
        fs::create_dir(&directory).expect("create test directory");
        let path = directory.join("connections.toml");
        atomic_write(&path, b"first").expect("first write");
        atomic_write(&path, b"second").expect("second write");
        assert_eq!(fs::read(&path).expect("current"), b"second");
        assert_eq!(
            fs::read(path.with_extension("toml.bak")).expect("backup"),
            b"first"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }

    #[test]
    fn restoring_backup_does_not_replace_it_with_corrupt_primary() {
        let directory = std::env::temp_dir().join(format!("beam-recovery-{}", Uuid::new_v4()));
        fs::create_dir(&directory).expect("create test directory");
        let path = directory.join("connections.toml");
        atomic_write(&path, b"valid = true").expect("initial write");
        atomic_write(&path, b"new = true").expect("create backup");
        fs::write(&path, b"invalid = [").expect("simulate partial file");
        let backup = fs::read(backup_path(&path)).expect("read backup");
        restore_backup(&path, &backup).expect("restore backup");
        assert_eq!(fs::read(&path).expect("primary"), b"valid = true");
        assert_eq!(
            fs::read(backup_path(&path)).expect("backup"),
            b"valid = true"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }
}
