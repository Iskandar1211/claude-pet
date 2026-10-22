use anyhow::{Context, Result};
use serde::Serialize;

use crate::registry::SessionRecord;

#[derive(Serialize)]
struct PermissionBody<'a> {
    request_id: &'a str,
    behavior: &'a str,
}

/// Токен из `channels/<pid>.json`, иначе глобальный из config.json (старый Channel).
pub fn channel_token_for<'a>(session: &'a SessionRecord, global: &'a str) -> &'a str {
    session.channel_token.as_deref().unwrap_or(global)
}

pub async fn send_channel_message(
    client: &reqwest::Client,
    port: u16,
    token: &str,
    text: &str,
) -> Result<()> {
    let url = format!("http://127.0.0.1:{port}/");
    client
        .post(url)
        .header("X-Pet-Token", token)
        .header("Host", format!("127.0.0.1:{port}"))
        .body(text.to_string())
        .send()
        .await
        .context("channel POST")?
        .error_for_status()
        .context("channel status")?;
    Ok(())
}

pub async fn send_channel_permission(
    client: &reqwest::Client,
    port: u16,
    token: &str,
    request_id: &str,
    allow: bool,
) -> Result<()> {
    let url = format!("http://127.0.0.1:{port}/permission");
    let body = PermissionBody {
        request_id,
        behavior: if allow { "allow" } else { "deny" },
    };
    client
        .post(url)
        .header("X-Pet-Token", token)
        .header("Host", format!("127.0.0.1:{port}"))
        .json(&body)
        .send()
        .await
        .context("channel permission POST")?
        .error_for_status()
        .context("channel permission status")?;
    Ok(())
}
