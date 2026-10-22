use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths::{config_path, pet_dir};

#[derive(Clone, Serialize, Deserialize)]
pub struct PetConfig {
    pub channel_token: String,
}

pub fn load_or_create_config() -> Result<PetConfig> {
    let dir = pet_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir).context("create pet dir")?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }

    let path = config_path();
    if path.exists() {
        let raw = fs::read_to_string(&path).context("read config")?;
        return Ok(serde_json::from_str(&raw).context("parse config")?);
    }

    let cfg = PetConfig {
        channel_token: random_token(),
    };
    match write_config_new(&path, &cfg) {
        Ok(()) => Ok(cfg),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let raw = fs::read_to_string(&path).context("read config after race")?;
            serde_json::from_str(&raw).context("parse config after race")
        }
        Err(e) => Err(e.into()),
    }
}

fn write_config_new(path: &Path, cfg: &PetConfig) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(cfg).map_err(|e| std::io::Error::other(e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(json.as_bytes())?;
        return Ok(());
    }
    #[cfg(not(unix))]
    {
        fs::write(path, json)?;
        Ok(())
    }
}

fn random_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::getrandom(&mut buf).expect("getrandom");
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}
