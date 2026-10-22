//! `pet-bridge serve`: протокол для Swift-приложения (bridge/PROTOCOL.md).
//! stdout — только строки протокола, логи — в stderr. EOF на stdin → выход.

use std::io::Write;
use std::time::Duration;

use anyhow::{Context, Result};
use pet_bridge::protocol::{
    pet_status, sessions_snapshot, Command, Event, Request, SessionDto, HISTORY_MAX_LIMIT,
    PROTOCOL_VERSION,
};
use pet_bridge::{
    build_http_client, channel_token_for, clear_pending_file, load_or_create_config, read_chat, read_usage,
    resolve_session_id, send_channel_permission, send_to_session_confirmed, usage_path, Bridge, DeliveryResult,
    DeliveryRoute, PetConfig, Usage,
};
use tokio::io::{AsyncBufReadExt, BufReader};

pub async fn run() -> Result<()> {
    emit(&Event::Hello { protocol: PROTOCOL_VERSION })?;
    let mut last_usage: Option<Usage> = None;
    publish_usage_if_changed(&mut last_usage)?;

    let cfg = load_or_create_config()?;
    let client = build_http_client()?;
    let mut bridge = Bridge::new();
    bridge.watch_channels()?;

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut last: Option<Vec<SessionDto>> = None;

    loop {
        tokio::select! {
            _ = tick.tick() => {}
            line = lines.next_line() => {
                let Some(line) = line.context("read stdin")? else { break };
                if let Some(event) = handle(&line, &mut bridge, &cfg, &client).await {
                    emit(&event)?;
                }
            }
        }
        // После команды тоже: снятый pending должен уйти в приложение сразу, а не через секунду.
        match bridge.refresh() {
            Ok(()) => publish_if_changed(&bridge, &mut last)?,
            Err(e) => tracing::warn!("refresh: {e:#}"),
        }
        publish_usage_if_changed(&mut last_usage)?;
    }
    Ok(())
}

fn publish_if_changed(bridge: &Bridge, last: &mut Option<Vec<SessionDto>>) -> Result<()> {
    let sessions = sessions_snapshot(bridge.registry());
    if last.as_ref() == Some(&sessions) {
        return Ok(());
    }
    emit(&Event::Sessions {
        pet_status: pet_status(&sessions),
        sessions: sessions.clone(),
    })?;
    *last = Some(sessions);
    Ok(())
}

/// Файл в сотню байт — читаем на каждом тике, как и остальное состояние моста.
fn publish_usage_if_changed(last: &mut Option<Usage>) -> Result<()> {
    let Some(usage) = read_usage(&usage_path()) else { return Ok(()) };
    if last.as_ref() == Some(&usage) {
        return Ok(());
    }
    emit(&Event::Usage {
        five_hour_pct: usage.five_hour_pct,
        resets_at: usage.resets_at,
        updated_at: usage.updated_at,
    })?;
    *last = Some(usage);
    Ok(())
}

async fn handle(line: &str, bridge: &mut Bridge, cfg: &PetConfig, client: &reqwest::Client) -> Option<Event> {
    if line.trim().is_empty() {
        return None;
    }
    let request: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("bad command: {e}");
            // Ответить можно, только если удалось достать id.
            let id = serde_json::from_str::<serde_json::Value>(line)
                .ok()?
                .get("id")?
                .as_str()?
                .to_string();
            return Some(failure(id, format!("неверная команда: {e}")));
        }
    };
    let id = request.id;
    if let Command::History { session_id, limit } = request.command {
        return Some(match history(bridge, &id, &session_id, limit).await {
            Ok(event) => event,
            Err(e) => failure(id, format!("{e:#}")),
        });
    }
    let outcome = match request.command {
        Command::Send { session_id, text } => send(bridge, cfg, client, &session_id, &text).await,
        Command::Permission { session_id, request_id, allow } => {
            permission(bridge, cfg, client, &session_id, &request_id, allow).await
        }
        Command::History { .. } => unreachable!("history обработан выше"),
    };
    Some(match outcome {
        Ok((route, delivery)) => Event::Result { id, ok: true, route, delivery, error: None },
        Err(e) => failure(id, format!("{e:#}")),
    })
}

type Outcome = Result<(Option<&'static str>, Option<&'static str>)>;

async fn send(bridge: &Bridge, cfg: &PetConfig, client: &reqwest::Client, session: &str, text: &str) -> Outcome {
    let sid = resolve_session_id(bridge.registry(), session)?;
    let rec = bridge.registry().get(&sid).context("сессия пропала из реестра")?;
    let (route, result) = send_to_session_confirmed(client, &cfg.channel_token, rec, text).await?;
    let route = match route {
        DeliveryRoute::Channel => "channel",
        DeliveryRoute::Inbox => "inbox",
    };
    let delivery = match result {
        DeliveryResult::Sent => "sent",
        DeliveryResult::Confirmed => "confirmed",
        DeliveryResult::Unconfirmed => "unconfirmed",
    };
    Ok((Some(route), Some(delivery)))
}

async fn history(bridge: &Bridge, id: &str, session: &str, limit: usize) -> Result<Event> {
    let sid = resolve_session_id(bridge.registry(), session)?;
    let rec = bridge.registry().get(&sid).context("сессия пропала из реестра")?;
    let path = rec.transcript_path.clone().context("у сессии нет транскрипта")?;
    let limit = limit.min(HISTORY_MAX_LIMIT);
    // Чтение файла — блокирующее, не держим им цикл обработки stdin.
    let messages = tokio::task::spawn_blocking(move || read_chat(&path, limit))
        .await
        .context("history task")?
        .context("чтение транскрипта")?;
    Ok(Event::History { id: id.to_string(), session_id: sid, messages })
}

async fn permission(
    bridge: &mut Bridge,
    cfg: &PetConfig,
    client: &reqwest::Client,
    session: &str,
    request_id: &str,
    allow: bool,
) -> Outcome {
    let sid = resolve_session_id(bridge.registry(), session)?;
    let rec = bridge.registry().get(&sid).context("сессия пропала из реестра")?;
    let port = rec.channel_port.context("у сессии нет Channel (запусти claude-pet)")?;
    let token = channel_token_for(rec, &cfg.channel_token);
    send_channel_permission(client, port, token, request_id, allow).await?;
    clear_pending_file(&sid);
    bridge.clear_pending_permission(&sid);
    Ok((None, None))
}

fn failure(id: String, error: String) -> Event {
    Event::Result { id, ok: false, route: None, delivery: None, error: Some(error) }
}

/// Ошибка записи (приложение закрыло stdout) завершает serve, а не роняет его паникой println!.
fn emit(event: &Event) -> Result<()> {
    let line = serde_json::to_string(event).context("serialize event")?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{line}").context("write stdout")?;
    out.flush().context("flush stdout")
}
