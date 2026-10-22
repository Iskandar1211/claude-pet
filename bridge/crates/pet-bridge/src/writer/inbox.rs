use std::io::Write;
use std::os::unix::net::UnixStream;

use anyhow::{Context, Result};
use serde_json::json;

/// Единственный модуль, знающий недокументированный протокол inbox-сокета.
pub fn send_inbox(socket: &str, token: &str, text: &str) -> Result<()> {
    let mut stream = UnixStream::connect(socket).context("connect inbox socket")?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(3)))?;

    let auth = json!({"type": "auth", "token": token});
    let user = json!({
        "type": "user",
        "message": {"role": "user", "content": text}
    });
    for line in [auth, user] {
        let mut payload = serde_json::to_string(&line)?;
        payload.push('\n');
        stream
            .write_all(payload.as_bytes())
            .context("write inbox line")?;
    }
    Ok(())
}
