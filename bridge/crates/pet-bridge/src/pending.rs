use std::fs;

use anyhow::{Context, Result};

use std::path::Path;

use crate::paths::{pending_dir, pending_path, pending_path_in};
use crate::state::PendingPermission;

pub fn save_pending(session_id: &str, perm: &PendingPermission) -> Result<()> {
    save_pending_in(&pending_dir(), session_id, perm)
}

pub(crate) fn save_pending_in(dir: &Path, session_id: &str, perm: &PendingPermission) -> Result<()> {
    let path = pending_path_in(dir, session_id).context("invalid session id")?;
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
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
    load_pending_in(&pending_dir(), session_id)
}

pub(crate) fn load_pending_in(dir: &Path, session_id: &str) -> Option<PendingPermission> {
    let path = pending_path_in(dir, session_id)?;
    let raw = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn clear_pending_file(session_id: &str) {
    if let Some(path) = pending_path(session_id) {
        let _ = fs::remove_file(path);
    }
}
