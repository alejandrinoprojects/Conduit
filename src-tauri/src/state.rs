use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::sync::Arc;
use tokio::process::{Child, ChildStdin};
use tokio::sync::Mutex;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct MinecraftProfile {
    pub uuid: String,
    pub username: String,
    pub access_token: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum ServerStatus {
    Stopped,
    Starting,
    Online,
    Error,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum ClientStatus {
    Stopped,
    Downloading,
    Playing,
}

pub struct InnerState {
    pub auth_profile: Option<MinecraftProfile>,
    pub server_status: ServerStatus,
    pub client_status: ClientStatus,
    pub tunnel_url: Option<String>,
    pub tunnel_claim_url: Option<String>,
    pub server_stdin: Option<ChildStdin>,
    pub server_child: Option<Child>,
    pub tunnel_child: Option<Child>,
}

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<Mutex<InnerState>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(InnerState {
                auth_profile: None,
                server_status: ServerStatus::Stopped,
                client_status: ClientStatus::Stopped,
                tunnel_url: None,
                tunnel_claim_url: None,
                server_stdin: None,
                server_child: None,
                tunnel_child: None,
            })),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for AppState {
    type Target = Arc<Mutex<InnerState>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub type ProcessState = AppState;
