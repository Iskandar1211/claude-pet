use std::collections::HashMap;

use anyhow::Result;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use crate::channel_events::{run_sse_client, SessionChannelEvent};
use crate::config::load_or_create_config;
use crate::http_client::build_http_client;
use crate::paths::events_log;
use crate::registry::SessionRegistry;

pub struct Bridge {
    registry: SessionRegistry,
    listener: Option<ChannelListener>,
}

/// SSE-задачи по сессиям: session_id → (порт, токен, задача).
struct ChannelListener {
    client: reqwest::Client,
    /// Глобальный токен из config.json — только если в `channels/<pid>.json` токена нет.
    token: String,
    tx: UnboundedSender<SessionChannelEvent>,
    rx: UnboundedReceiver<SessionChannelEvent>,
    tasks: HashMap<String, (u16, String, JoinHandle<()>)>,
}

impl ChannelListener {
    fn sync(&mut self, registry: &SessionRegistry) {
        let live: HashMap<&str, (u16, &str)> = registry
            .sessions()
            .filter_map(|s| {
                let token = s.channel_token.as_deref().unwrap_or(&self.token);
                s.channel_port.map(|p| (s.session_id.as_str(), (p, token)))
            })
            .collect();
        self.tasks.retain(|id, (port, token, handle)| {
            let keep = live
                .get(id.as_str())
                .is_some_and(|(p, t)| p == port && *t == token.as_str());
            if !keep {
                handle.abort();
            }
            keep
        });
        for (id, (port, token)) in live {
            if self.tasks.contains_key(id) {
                continue;
            }
            let handle = tokio::spawn(run_sse_client(
                self.client.clone(),
                port,
                token.to_string(),
                id.to_string(),
                self.tx.clone(),
            ));
            self.tasks
                .insert(id.to_string(), (port, token.to_string(), handle));
        }
    }

    fn drain(&mut self, registry: &mut SessionRegistry) {
        while let Ok(ev) = self.rx.try_recv() {
            registry.apply_channel_event(ev);
        }
    }
}

impl Drop for ChannelListener {
    fn drop(&mut self) {
        for (_, _, handle) in self.tasks.values() {
            handle.abort();
        }
    }
}

impl Bridge {
    pub fn new() -> Self {
        Self {
            registry: SessionRegistry::new(),
            listener: None,
        }
    }

    /// Включает SSE-клиенты Channel: `refresh()` сам подключается к `channel_port` каждой сессии.
    /// Вызывать внутри tokio runtime.
    pub fn watch_channels(&mut self) -> Result<()> {
        let cfg = load_or_create_config()?;
        let (tx, rx) = unbounded_channel();
        self.listener = Some(ChannelListener {
            client: build_http_client()?,
            token: cfg.channel_token,
            tx,
            rx,
            tasks: HashMap::new(),
        });
        Ok(())
    }

    pub fn registry(&self) -> &SessionRegistry {
        &self.registry
    }

    /// После вердикта из UI/Tauri: снять pending и файл `~/.claude-pet/pending/`.
    pub fn clear_pending_permission(&mut self, session_id: &str) {
        self.registry.clear_pending_permission(session_id);
    }

    pub fn refresh(&mut self) -> Result<()> {
        let path = events_log();
        self.registry.refresh(&path)?;
        if let Some(listener) = self.listener.as_mut() {
            listener.sync(&self.registry);
            listener.drain(&mut self.registry);
        }
        Ok(())
    }
}
