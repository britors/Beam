//! Events emitted by an active session, consumed by whatever frontend is driving it.
//!
//! The frontend never touches IronRDP types directly: everything crossing the core/frontend
//! boundary here is a plain, GTK-agnostic type. Interactive prompts (certificate confirmation,
//! missing password) carry a `oneshot::Sender` so the reply travels back through the same event
//! without a second, out-of-band channel.

use tokio::sync::oneshot;

use crate::known_hosts::{Fingerprint, TrustDecision};
use crate::session::framebuffer::DirtyRect;

/// Why a session ended.
#[derive(Debug, Clone)]
pub enum DisconnectReason {
    /// The user asked to disconnect.
    UserInitiated,
    /// The server closed the connection or a network error occurred.
    ConnectionLost(String),
    /// The connection handshake itself failed (bad credentials, negotiation failure, ...).
    ConnectionFailed(String),
}

impl std::fmt::Display for DisconnectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserInitiated => write!(f, "desconectado pelo usuário"),
            Self::ConnectionLost(msg) => write!(f, "conexão perdida: {msg}"),
            Self::ConnectionFailed(msg) => write!(f, "falha na conexão: {msg}"),
        }
    }
}

/// A request to confirm the server's TLS certificate, TOFU-style.
#[derive(Debug)]
pub struct CertPromptRequest {
    pub address: String,
    pub fingerprint: Fingerprint,
    pub decision: TrustDecision,
    pub respond: oneshot::Sender<bool>,
}

/// A request for the account password, sent when nothing usable was found in the Secret Service.
#[derive(Debug)]
pub struct CredentialsPromptRequest {
    pub username: String,
    /// The frontend should send `Some((password, save_to_keyring))`, or `None` to cancel.
    pub respond: oneshot::Sender<Option<(String, bool)>>,
}

#[derive(Debug)]
pub enum SessionEvent {
    /// The RDP handshake completed and the desktop is ready to be displayed.
    Connected { width: u16, height: u16 },
    /// New pixels are available in the session's [`crate::session::framebuffer::Framebuffer`].
    FramebufferUpdated { rect: DirtyRect },
    /// The server's certificate must be confirmed before the connection can proceed.
    CertPrompt(CertPromptRequest),
    /// A password is required to continue authenticating.
    CredsNeeded(CredentialsPromptRequest),
    /// Text was copied to the remote clipboard and is now available locally.
    ClipboardTextReceived(String),
    /// The session ended, gracefully or not.
    Disconnected(DisconnectReason),
}
