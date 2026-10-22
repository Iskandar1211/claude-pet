use std::path::Path;

use crate::paths::pid_from_socket;

pub fn socket_alive(socket: &str) -> bool {
    Path::new(socket).exists()
}

pub fn process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn session_alive(socket: Option<&str>) -> bool {
    match socket {
        Some(s) if !s.is_empty() => {
            if !socket_alive(s) {
                return false;
            }
            if let Some(pid) = pid_from_socket(s) {
                return process_alive(pid);
            }
            true
        }
        _ => false,
    }
}
