use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::sync::mpsc::UnboundedSender;

use crate::state::PendingPermission;

#[derive(Debug)]
pub enum ChannelEvent {
    Permission(PendingPermission),
    Reply { text: String },
}

#[derive(Debug)]
pub struct SessionChannelEvent {
    pub session_id: String,
    pub event: ChannelEvent,
}

#[derive(Deserialize)]
struct PermissionData {
    request_id: String,
    tool_name: String,
    #[serde(default)]
    input_preview: String,
}

#[derive(Deserialize)]
struct ReplyData {
    text: String,
}

/// SSE-клиент одной сессии: `GET /events` с переподключением, пока задачу не abort'нут.
pub async fn run_sse_client(
    client: reqwest::Client,
    port: u16,
    token: String,
    session_id: String,
    tx: UnboundedSender<SessionChannelEvent>,
) {
    loop {
        if let Err(err) = read_stream(&client, port, &token, &session_id, &tx).await {
            tracing::debug!("channel SSE {session_id}: {err:#}");
        }
        if tx.is_closed() {
            return;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn read_stream(
    client: &reqwest::Client,
    port: u16,
    token: &str,
    session_id: &str,
    tx: &UnboundedSender<SessionChannelEvent>,
) -> Result<()> {
    let mut resp = client
        .get(format!("http://127.0.0.1:{port}/events"))
        .header("X-Pet-Token", token)
        .header("Host", format!("127.0.0.1:{port}"))
        .send()
        .await
        .context("channel SSE GET")?
        .error_for_status()
        .context("channel SSE status")?;

    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = resp.chunk().await.context("channel SSE chunk")? {
        buf.extend_from_slice(&chunk);
        while let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
            let block: Vec<u8> = buf.drain(..pos + 2).collect();
            if let Some(event) = parse_block(&String::from_utf8_lossy(&block)) {
                let msg = SessionChannelEvent {
                    session_id: session_id.to_string(),
                    event,
                };
                if tx.send(msg).is_err() {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

fn parse_block(block: &str) -> Option<ChannelEvent> {
    let mut name = None;
    let mut data = String::new();
    for line in block.lines() {
        if let Some(v) = line.strip_prefix("event:") {
            name = Some(v.trim());
        } else if let Some(v) = line.strip_prefix("data:") {
            data.push_str(v.trim_start());
        }
    }
    match name? {
        "permission" => {
            let d: PermissionData = serde_json::from_str(&data).ok()?;
            Some(ChannelEvent::Permission(PendingPermission {
                request_id: d.request_id,
                tool_name: d.tool_name,
                input_preview: d.input_preview,
            }))
        }
        "reply" => {
            let d: ReplyData = serde_json::from_str(&data).ok()?;
            Some(ChannelEvent::Reply { text: d.text })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_permission() {
        let raw = "event: permission\ndata: {\"request_id\":\"abc\",\"tool_name\":\"Bash\",\"description\":\"d\",\"input_preview\":\"ls\"}\n\n";
        match parse_block(raw) {
            Some(ChannelEvent::Permission(p)) => {
                assert_eq!(p.request_id, "abc");
                assert_eq!(p.tool_name, "Bash");
                assert_eq!(p.input_preview, "ls");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_reply_and_skips_comment() {
        let raw = "event: reply\ndata: {\"chat_id\":\"1\",\"text\":\"hi\"}\n\n";
        assert!(matches!(
            parse_block(raw),
            Some(ChannelEvent::Reply { text }) if text == "hi"
        ));
        assert!(parse_block(": connected\n\n").is_none());
    }
}
