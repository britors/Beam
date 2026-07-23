//! Public API surface of the session engine: [`connect`] starts a session and hands back a
//! [`SessionController`] (commands + framebuffer) and [`SessionEvents`] (the event stream) — the
//! only things a frontend needs to drive an RDP session. No IronRDP type ever crosses this
//! boundary.

mod active;
mod clipboard;
pub mod framebuffer;
mod connector;
mod input;

use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tracing::warn;

pub use self::framebuffer::Framebuffer;
pub use self::input::{InputEvent, PointerButton};
pub use self::connector::ConnectError;

use self::clipboard::ClipboardBridge;
use crate::events::{CredentialsPromptRequest, DisconnectReason, SessionEvent};
use crate::profile::ConnectionProfile;
use crate::secrets::{self, SecretKey};

/// Commands a frontend can send into a running session. Internal plumbing only — a frontend
/// never constructs these directly, it goes through [`SessionHandle`]'s methods.
pub(crate) enum SessionCommand {
    Input(InputEvent),
    CtrlAltDel,
    LocalClipboardChanged,
    Disconnect,
}

/// The receiving half of a session: a move-only stream of [`SessionEvent`]s.
///
/// Kept separate from [`SessionController`] specifically so a frontend never needs to share a
/// single `SessionHandle` behind a `RefCell` to poll events from one task while sending commands
/// from UI callbacks on the same thread — doing so risks a `RefCell` borrow panic the moment a
/// callback fires while the event-pump task is suspended mid-`.await` holding a borrow. With the
/// split, the event pump owns its receiver outright and every other closure just clones the
/// cheap, `Send`-free [`SessionController`].
pub struct SessionEvents {
    events: mpsc::Receiver<SessionEvent>,
}

impl SessionEvents {
    /// Await the next session event. Returns `None` once the session task has fully exited
    /// (always preceded by a [`SessionEvent::Disconnected`]).
    pub async fn next_event(&mut self) -> Option<SessionEvent> {
        self.events.recv().await
    }
}

/// A cheaply-`Clone`-able handle for sending commands into a running session and reading its
/// framebuffer. See [`SessionEvents`] for why this is a separate type from the event stream.
#[derive(Clone)]
pub struct SessionController {
    commands: mpsc::UnboundedSender<SessionCommand>,
    framebuffer: Arc<Framebuffer>,
    clipboard_bridge: Arc<ClipboardBridge>,
}

impl SessionController {
    /// The session's shared framebuffer, for the display widget to snapshot when painting.
    pub fn framebuffer(&self) -> &Arc<Framebuffer> {
        &self.framebuffer
    }

    pub fn send_input(&self, event: InputEvent) {
        let _ = self.commands.send(SessionCommand::Input(event));
    }

    pub fn send_ctrl_alt_del(&self) {
        let _ = self.commands.send(SessionCommand::CtrlAltDel);
    }

    /// Notify the session that the local (GTK) clipboard now holds `text`, offering it to the
    /// remote desktop.
    pub fn set_local_clipboard_text(&self, text: String) {
        // Write the cache before enqueuing the command: the active-session loop only reads it
        // after it dequeues `LocalClipboardChanged`, and by then this write has already
        // happened-before that dequeue (it happened-before the very send below).
        self.clipboard_bridge.set_local_text(text);
        let _ = self.commands.send(SessionCommand::LocalClipboardChanged);
    }

    pub fn disconnect(&self) {
        let _ = self.commands.send(SessionCommand::Disconnect);
    }
}

/// Start connecting to `profile` in the background, on `runtime`. Returns immediately; watch
/// [`SessionEvents::next_event`] for [`SessionEvent::Connected`], [`SessionEvent::CertPrompt`],
/// [`SessionEvent::CredsNeeded`], and eventually [`SessionEvent::Disconnected`] if the attempt
/// fails.
pub fn connect(profile: ConnectionProfile, runtime: &tokio::runtime::Handle) -> (SessionController, SessionEvents) {
    let (events_tx, events_rx) = mpsc::channel(64);
    let (commands_tx, commands_rx) = mpsc::unbounded_channel();
    let framebuffer = Arc::new(Framebuffer::new(profile.resolution.width, profile.resolution.height));
    let clipboard_bridge = Arc::new(ClipboardBridge::default());

    let task_framebuffer = framebuffer.clone();
    let task_clipboard_bridge = clipboard_bridge.clone();
    runtime.spawn(async move {
        run_session(profile, events_tx, commands_rx, task_framebuffer, task_clipboard_bridge).await;
    });

    (
        SessionController {
            commands: commands_tx,
            framebuffer,
            clipboard_bridge,
        },
        SessionEvents { events: events_rx },
    )
}

async fn run_session(
    profile: ConnectionProfile,
    events: mpsc::Sender<SessionEvent>,
    commands: mpsc::UnboundedReceiver<SessionCommand>,
    framebuffer: Arc<Framebuffer>,
    clipboard_bridge: Arc<ClipboardBridge>,
) {
    let username = profile.username.clone();
    let key = SecretKey {
        host: &profile.host,
        port: profile.port,
        user: &username,
    };

    let password = match secrets::lookup_password(&key).await {
        Ok(Some(password)) => password,
        other => {
            if let Err(e) = other {
                warn!("falha ao consultar o chaveiro do sistema: {e}");
            }

            let (tx, rx) = oneshot::channel();
            let _ = events
                .send(SessionEvent::CredsNeeded(CredentialsPromptRequest {
                    username: username.clone(),
                    respond: tx,
                }))
                .await;

            match rx.await {
                Ok(Some((password, save))) => {
                    if save {
                        if let Err(e) = secrets::store_password(&key, &password).await {
                            warn!("falha ao salvar senha no chaveiro do sistema: {e}");
                        }
                    }
                    password
                }
                _ => {
                    let _ = events
                        .send(SessionEvent::Disconnected(DisconnectReason::ConnectionFailed(
                            "senha não fornecida".to_owned(),
                        )))
                        .await;
                    return;
                }
            }
        }
    };

    let (backend_tx, backend_rx) = mpsc::unbounded_channel();
    let backend = clipboard::build_backend(clipboard_bridge, backend_tx);
    let cliprdr = ironrdp_cliprdr::Cliprdr::<ironrdp_cliprdr::Client>::new(backend);

    match connector::connect(&profile, &username, &password, cliprdr, &events).await {
        Ok(connected) => {
            active::run(
                connected.framed,
                connected.connection_result,
                events,
                commands,
                backend_rx,
                framebuffer,
            )
            .await;
        }
        Err(e) => {
            let _ = events
                .send(SessionEvent::Disconnected(DisconnectReason::ConnectionFailed(e.to_string())))
                .await;
        }
    }
}
