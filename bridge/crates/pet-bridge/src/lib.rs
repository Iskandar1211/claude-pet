mod bridge;
mod chat;
mod channel_events;
mod config;
mod hook_event;
mod http_client;
pub mod install;
pub mod launcher;
mod liveness;
mod paths;
mod pending;
pub mod protocol;
mod registry;
mod state;
mod transcript;
mod usage;
mod writer;

pub use bridge::Bridge;
pub use config::{load_or_create_config, PetConfig};
pub use http_client::build_http_client;
pub use paths::{events_log, pet_dir, pid_from_socket, usage_path};
pub use pending::{clear_pending_file, load_pending};
pub use registry::{resolve_session_id, SessionRecord, SessionRegistry};
pub use state::SessionStatus;
pub use chat::read_chat;
pub use usage::{read_usage, Usage};
pub use writer::{
    channel_token_for, send_channel_permission, send_to_session, send_to_session_confirmed, DeliveryResult,
    DeliveryRoute,
};
