use std::fs;

use anyhow::{Context, Result};

use crate::paths::{pending_dir, pending_path};
use crate::state::PendingPermission;

pub fn save_pending(session_id: &str, perm: &PendingPermission) -> Result<()> {
    let path = pending_path(session_id).context("invalid session id")?;
    fs::create_dir_all(pending_dir())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&pending_dir(), fs::Permissions::from_mode(0o700));
    }
    let json = serde_json::to_string(perm)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn load_pending(session_id: &str) -> Option<PendingPermission> {
    let path = pending_path(session_id)?;
    let raw = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn clear_pending_file(session_id: &str) {
    if let Some(path) = pending_path(session_id) {
        let _ = fs::remove_file(path);
    }
}
