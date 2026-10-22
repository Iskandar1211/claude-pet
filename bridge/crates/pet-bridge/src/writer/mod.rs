mod channel;
mod confirm;
mod inbox;

use anyhow::Result;

use crate::registry::SessionRecord;

pub use channel::{channel_token_for, send_channel_message, send_channel_permission};
use confirm::{transcript_len, wait_for_echo};
pub use inbox::send_inbox;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryRoute {
    Channel,
    Inbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryResult {
    Sent,
    Confirmed,
    Unconfirmed,
}

pub async fn send_to_session(
    client: &reqwest::Client,
    channel_token: &str,
    session: &SessionRecord,
    text: &str,
) -> Result<(DeliveryRoute, DeliveryResult)> {
    if let Some(port) = session.channel_port {
        let token = channel_token_for(session, channel_token);
        send_channel_message(client, port, token, text).await?;
        return Ok((DeliveryRoute::Channel, DeliveryResult::Sent));
    }
    let socket = session
        .inbox_socket
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("нет inbox-сокета для сессии"))?;
    let token = session
        .inbox_token
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("нет inbox-токена для сессии"))?;
    send_inbox(socket, token, text)?;
    Ok((DeliveryRoute::Inbox, DeliveryResult::Sent))
}

/// `send_to_session` + до 10 с ожидания эха в транскрипте (если путь к нему известен).
pub async fn send_to_session_confirmed(
    client: &reqwest::Client,
    channel_token: &str,
    session: &SessionRecord,
    text: &str,
) -> Result<(DeliveryRoute, DeliveryResult)> {
    let transcript = session.transcript_path.as_deref();
    let start_len = transcript.map(transcript_len).unwrap_or(0);
    let (route, sent) = send_to_session(client, channel_token, session, text).await?;
    let result = match transcript {
        Some(path) => wait_for_echo(path, start_len, text).await,
        None => sent,
    };
    Ok((route, result))
}
