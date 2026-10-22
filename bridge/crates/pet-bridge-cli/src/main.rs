use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use pet_bridge::{
    build_http_client, channel_token_for, clear_pending_file, load_or_create_config, load_pending,
    resolve_session_id, send_channel_permission, send_to_session_confirmed, Bridge, SessionStatus,
};
use tracing_subscriber::EnvFilter;

mod serve;

#[derive(Parser)]
#[command(name = "pet-bridge", about = "Локальный мост Claude Code Pet (шаг 2)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Однократный список сессий
    List,
    /// Живой список (обновление раз в секунду)
    Watch,
    /// Отправить текст в сессию (id или префикс)
    Send {
        session: String,
        text: String,
    },
    /// Hooks питомца во всех проектах: ~/.claude/settings.json (с бэкапом)
    Hooks {
        #[arg(value_parser = ["install", "uninstall", "status"])]
        action: String,
    },
    /// Команда claude-pet в ~/.local/bin: Claude Code с Channel питомца из любой папки
    Launcher {
        #[arg(value_parser = ["install", "uninstall", "status"])]
        action: String,
        /// Абсолютный путь к pet-channel (для install)
        channel: Option<std::path::PathBuf>,
    },
    /// Протокол для Swift-приложения: JSON-строки в stdout/stdin (bridge/PROTOCOL.md)
    Serve,
    /// Вердикт для pending permission (только Channel)
    Approve {
        session: String,
        #[arg(value_parser = ["yes", "no", "y", "n"])]
        verdict: String,
        #[arg(long)]
        request_id: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        // stdout занят протоколом `serve` — логи только в stderr.
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::List => {
            let mut bridge = Bridge::new();
            bridge.refresh()?;
            print_sessions(&bridge);
        }
        Commands::Watch => {
            let mut bridge = Bridge::new();
            bridge.watch_channels()?;
            loop {
                bridge.refresh()?;
                print!("\x1b[2J\x1b[H");
                print_sessions(&bridge);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        Commands::Serve => serve::run().await?,
        Commands::Hooks { action } => hooks(&action)?,
        Commands::Launcher { action, channel } => launcher(&action, channel)?,
        Commands::Send { session, text } => {
            let cfg = load_or_create_config()?;
            let client = build_http_client()?;
            let mut bridge = Bridge::new();
            bridge.refresh()?;
            let sid = resolve_session_id(bridge.registry(), &session)?;
            let rec = bridge
                .registry()
                .get(&sid)
                .context("сессия пропала из реестра")?;
            let (route, result) =
                send_to_session_confirmed(&client, &cfg.channel_token, rec, &text).await?;
            println!(
                "route={} result={}",
                match route {
                    pet_bridge::DeliveryRoute::Channel => "channel",
                    pet_bridge::DeliveryRoute::Inbox => "inbox",
                },
                match result {
                    pet_bridge::DeliveryResult::Sent => "sent",
                    pet_bridge::DeliveryResult::Confirmed => "confirmed",
                    pet_bridge::DeliveryResult::Unconfirmed => "unconfirmed",
                }
            );
        }
        Commands::Approve {
            session,
            verdict,
            request_id,
        } => {
            let cfg = load_or_create_config()?;
            let client = build_http_client()?;
            let mut bridge = Bridge::new();
            bridge.refresh()?;
            let sid = resolve_session_id(bridge.registry(), &session)?;
            let rec = bridge
                .registry()
                .get(&sid)
                .context("сессия пропала из реестра")?;
            let port = rec
                .channel_port
                .context("у сессии нет Channel (запусти claude-pet)")?;
            let rid = request_id
                .or_else(|| {
                    rec.pending_permission
                        .as_ref()
                        .map(|p| p.request_id.clone())
                })
                .or_else(|| load_pending(&sid).map(|p| p.request_id))
                .context("нет request_id — запусти `pet-bridge watch` или передай --request-id")?;
            let allow = matches!(verdict.as_str(), "y" | "yes");
            let token = channel_token_for(rec, &cfg.channel_token);
            send_channel_permission(&client, port, token, &rid, allow).await?;
            clear_pending_file(&sid);
            println!("verdict sent");
        }
    }
    Ok(())
}

fn print_sessions(bridge: &Bridge) {
    println!(
        "{:<10} {:<14} {:<8} {:<8} {}",
        "session", "entrypoint", "status", "channel", "last"
    );
    let mut rows: Vec<_> = bridge.registry().sessions().collect();
    rows.sort_by_key(|s| s.session_id.clone());
    for s in rows {
        let short = s.session_id.chars().take(8).collect::<String>();
        let ep = s.entrypoint.as_deref().unwrap_or("-");
        let st = status_label(s.status);
        let ch = if s.channel_port.is_some() { "yes" } else { "no" };
        let last = match &s.pending_permission {
            Some(p) => format!("permission: {} {}", p.tool_name, p.input_preview),
            None => s.last_message.clone().unwrap_or_else(|| "-".into()),
        };
        println!("{:<10} {:<14} {:<8} {:<8} {}", short, ep, st, ch, last);
    }
}

fn status_label(s: SessionStatus) -> &'static str {
    match s {
        SessionStatus::Idle => "idle",
        SessionStatus::Working => "working",
        SessionStatus::WaitingPermission => "waiting",
        SessionStatus::Done => "done",
    }
}

/// Скрипт hook'а вшит в бинарник — установленная версия совпадает с репозиторием.
const HOOK_SCRIPT: &str = include_str!("../../../../prototype/hooks/log-event.sh");
const STATUSLINE_SCRIPT: &str = include_str!("../../../../prototype/hooks/statusline.sh");

/// `status` печатает одно слово (`installed` | `partial` | `not_installed`) — его читает приложение.
fn hooks(action: &str) -> Result<()> {
    use pet_bridge::install::{self, HooksStatus, Paths};
    let paths = Paths::user();
    let outcome = match action {
        "install" => {
            // Оба скрипта держатся на jq; без него hooks молча ничего не пишут и питомец слеп.
            let has_jq = std::process::Command::new("/bin/sh")
                .args(["-c", "command -v jq"])
                .output()
                .is_ok_and(|o| o.status.success());
            if !has_jq {
                anyhow::bail!("нужен jq: в macOS 15+ он есть в системе, на macOS 14 — `brew install jq`");
            }
            install::install(&paths, HOOK_SCRIPT, STATUSLINE_SCRIPT)?
        }
        "uninstall" => install::uninstall(&paths)?,
        _ => {
            let word = match install::current_status(&paths)? {
                HooksStatus::Installed => "installed",
                HooksStatus::Partial => "partial",
                HooksStatus::NotInstalled => "not_installed",
            };
            println!("{word}");
            return Ok(());
        }
    };
    match (outcome.changed, outcome.backup) {
        (false, _) => println!("{}: без изменений", paths.settings.display()),
        (true, Some(b)) => println!("{}: изменён, бэкап {}", paths.settings.display(), b.display()),
        (true, None) => println!("{}: создан", paths.settings.display()),
    }
    Ok(())
}

/// `status` печатает `installed` | `not_installed`.
fn launcher(action: &str, channel: Option<std::path::PathBuf>) -> Result<()> {
    use pet_bridge::launcher::{self, LauncherPaths};
    let paths = LauncherPaths::user();
    match action {
        "install" => {
            let channel = channel.context("нужен путь к pet-channel")?;
            launcher::install(&paths, &channel)?;
            println!("Команда {} установлена", paths.launcher.display());
        }
        "uninstall" => {
            launcher::uninstall(&paths)?;
            println!("Команда {} удалена", paths.launcher.display());
        }
        _ => println!("{}", if launcher::is_installed(&paths) { "installed" } else { "not_installed" }),
    }
    Ok(())
}
