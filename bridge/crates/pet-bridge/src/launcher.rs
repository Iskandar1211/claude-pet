//! Команда `claude-pet`: Claude Code с Channel питомца (ответы и одобрения из Quick Chat) из любой
//! папки. Channel — исполняемый файл внутри Claude Pet.app; путь к нему — в `~/.claude-pet/pet-mcp.json`,
//! который передаётся `claude --mcp-config`. Чужой `claude-pet` (без нашей метки) не перезаписываем.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::json;

/// По этой строке узнаём свой запускатель.
const MARKER: &str = "# claude-pet: ставит Claude Pet";

pub struct LauncherPaths {
    pub mcp_config: PathBuf,
    pub launcher: PathBuf,
}

impl LauncherPaths {
    /// `~/.claude-pet/pet-mcp.json` и `~/.local/bin/claude-pet`.
    pub fn user() -> Self {
        Self {
            mcp_config: crate::paths::pet_dir().join("pet-mcp.json"),
            launcher: crate::paths::dirs_home().join(".local").join("bin").join("claude-pet"),
        }
    }
}

fn launcher_script(mcp_config: &Path) -> String {
    format!(
        "#!/bin/sh\n{MARKER}. Claude Code с Channel питомца: ответы и одобрения из Quick Chat.\n\
         exec claude --mcp-config \"{}\" --dangerously-load-development-channels server:pet \"$@\"\n",
        mcp_config.display()
    )
}

fn is_ours(launcher: &Path) -> bool {
    fs::read_to_string(launcher).is_ok_and(|text| text.contains(MARKER))
}

pub fn is_installed(paths: &LauncherPaths) -> bool {
    is_ours(&paths.launcher) && paths.mcp_config.exists()
}

/// `channel` — абсолютный путь к `pet-channel` внутри .app. Переустановка обновляет путь (приложение переехало).
pub fn install(paths: &LauncherPaths, channel: &Path) -> Result<()> {
    if !channel.is_absolute() || !channel.exists() {
        bail!("Channel не найден: {}", channel.display());
    }
    if paths.launcher.exists() && !is_ours(&paths.launcher) {
        bail!("{} уже есть и он не от Claude Pet — не перезаписываю", paths.launcher.display());
    }
    let config = json!({ "mcpServers": { "pet": { "command": channel } } });
    write_with_mode(&paths.mcp_config, &serde_json::to_string_pretty(&config)?, 0o600)?;
    write_with_mode(&paths.launcher, &launcher_script(&paths.mcp_config), 0o755)?;
    Ok(())
}

/// Убирает только свой запускатель и конфиг.
pub fn uninstall(paths: &LauncherPaths) -> Result<()> {
    if is_ours(&paths.launcher) {
        fs::remove_file(&paths.launcher).with_context(|| format!("удаление {}", paths.launcher.display()))?;
    }
    let _ = fs::remove_file(&paths.mcp_config);
    Ok(())
}

fn write_with_mode(path: &Path, text: &str, mode: u32) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("папка {}", dir.display()))?;
    }
    fs::write(path, text).with_context(|| format!("запись {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).with_context(|| format!("права {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_uninstall_and_foreign_launcher() {
        let dir = std::env::temp_dir().join(format!("pet-launcher-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let paths = LauncherPaths {
            mcp_config: dir.join(".claude-pet/pet-mcp.json"),
            launcher: dir.join(".local/bin/claude-pet"),
        };
        let channel = dir.join("Claude Pet.app/Contents/MacOS/pet-channel");
        fs::create_dir_all(channel.parent().unwrap()).unwrap();
        fs::write(&channel, "bin").unwrap();

        assert!(install(&paths, Path::new("relative/pet-channel")).is_err());
        install(&paths, &channel).unwrap();
        assert!(is_installed(&paths));
        let config: serde_json::Value = serde_json::from_str(&fs::read_to_string(&paths.mcp_config).unwrap()).unwrap();
        assert_eq!(config["mcpServers"]["pet"]["command"], channel.display().to_string());
        let script = fs::read_to_string(&paths.launcher).unwrap();
        assert!(script.contains(&format!("--mcp-config \"{}\"", paths.mcp_config.display())));
        assert!(script.contains("server:pet \"$@\""));
        install(&paths, &channel).unwrap(); // повторно — без ошибок

        uninstall(&paths).unwrap();
        assert!(!paths.launcher.exists() && !paths.mcp_config.exists());

        // Свой claude-pet пользователя не трогаем ни при установке, ни при удалении.
        fs::write(&paths.launcher, "#!/bin/sh\necho mine\n").unwrap();
        assert!(install(&paths, &channel).is_err());
        uninstall(&paths).unwrap();
        assert_eq!(fs::read_to_string(&paths.launcher).unwrap(), "#!/bin/sh\necho mine\n");
        fs::remove_dir_all(&dir).ok();
    }
}
