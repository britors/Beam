//! Connection sequence: TCP → TLS → TOFU certificate check → NLA/CredSSP (or plain TLS) →
//! RDP finalization. Produces a ready-to-run active session.

use ironrdp_connector::sspi::generator::NetworkRequest;
use ironrdp_connector::{ClientConnector, ConnectionResult, ConnectorError, ConnectorResult, Credentials, ServerName};
use ironrdp_pdu::rdp::capability_sets::{client_codecs_capabilities, MajorPlatformType};
use ironrdp_pdu::rdp::client_info::{PerformanceFlags, TimezoneInfo};
use ironrdp_tls::TlsStream;
use ironrdp_tokio::{Framed, TokioFramed};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};

use crate::events::{CertPromptRequest, SessionEvent};
use crate::known_hosts::{self, Fingerprint, TrustDecision};
use crate::profile::ConnectionProfile;

/// A ready-to-drive session: the finalized handshake result plus the framed, encrypted stream.
pub(crate) struct Connected {
    pub connection_result: ConnectionResult,
    pub framed: Framed<ironrdp_tokio::TokioStream<TlsStream<TcpStream>>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error("falha de rede: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Connector(#[from] ConnectorError),
    #[error("certificado do servidor recusado pelo usuário")]
    CertificateRejected,
    #[error("conexão cancelada pelo usuário")]
    Cancelled,
}

/// A no-op [`ironrdp_tokio::NetworkClient`]: CredSSP only needs real network access for Kerberos
/// / KDC-proxy authentication, which Beam does not support (NTLM-over-CredSSP is negotiated
/// instead, which never suspends the generator for a network round-trip).
struct NoNetworkClient;

impl ironrdp_tokio::NetworkClient for NoNetworkClient {
    async fn send(&mut self, _request: &NetworkRequest) -> ConnectorResult<Vec<u8>> {
        Err(ironrdp_connector::general_err!(
            "solicitação de rede não suportada (somente NTLM via CredSSP é suportado)"
        ))
    }
}

fn build_config(profile: &ConnectionProfile, username: &str, password: &str) -> ironrdp_connector::Config {
    let bitmap = ironrdp_connector::BitmapConfig {
        color_depth: profile.color_depth,
        lossy_compression: true,
        codecs: client_codecs_capabilities(&[]).expect("baseline bitmap codecs are always valid"),
    };

    ironrdp_connector::Config {
        desktop_size: ironrdp_connector::DesktopSize {
            width: profile.resolution.width,
            height: profile.resolution.height,
        },
        desktop_scale_factor: 0,
        enable_tls: true,
        enable_credssp: true,
        credentials: Credentials::UsernamePassword {
            username: username.to_owned(),
            password: password.to_owned(),
        },
        domain: profile.domain.clone(),
        client_build: 19045,
        client_name: "beam".to_owned(),
        keyboard_type: ironrdp_pdu::gcc::KeyboardType::IbmEnhanced,
        keyboard_subtype: 0,
        keyboard_functional_keys_count: 12,
        keyboard_layout: 0,
        ime_file_name: String::new(),
        bitmap: Some(bitmap),
        dig_product_id: String::new(),
        client_dir: String::new(),
        alternate_shell: String::new(),
        work_dir: String::new(),
        platform: MajorPlatformType::UNSPECIFIED,
        hardware_id: None,
        request_data: None,
        autologon: false,
        enable_audio_playback: false,
        performance_flags: PerformanceFlags::default(),
        license_cache: None,
        timezone_info: TimezoneInfo::default(),
        compression_type: Some(ironrdp_pdu::rdp::client_info::CompressionType::K64),
        enable_server_pointer: true,
        pointer_software_rendering: false,
        multitransport_flags: None,
    }
}

/// Ask the frontend to confirm the server's certificate (TOFU), blocking this task until it
/// replies. Returns an error if the user declines.
async fn confirm_certificate(
    address: &str,
    cert: &x509_cert::Certificate,
    events: &mpsc::Sender<SessionEvent>,
) -> Result<(), ConnectError> {
    use x509_cert::der::Encode as _;

    let der = cert
        .to_der()
        .map_err(|e| ConnectError::Connector(ironrdp_connector::custom_err!("recodificar certificado do servidor", std::io::Error::other(e.to_string()))))?;
    let fingerprint = Fingerprint::of_der(&der);

    let decision = known_hosts::check(address, &fingerprint)
        .map_err(|e| ConnectError::Connector(ironrdp_connector::custom_err!("consultar known_hosts", std::io::Error::other(e.to_string()))))?;

    if decision == TrustDecision::Trusted {
        return Ok(());
    }

    let (tx, rx) = oneshot::channel();
    let _ = events
        .send(SessionEvent::CertPrompt(CertPromptRequest {
            address: address.to_owned(),
            fingerprint: fingerprint.clone(),
            decision,
            respond: tx,
        }))
        .await;

    let accepted = rx.await.unwrap_or(false);
    if !accepted {
        return Err(ConnectError::CertificateRejected);
    }

    known_hosts::trust(address, &fingerprint)
        .map_err(|e| ConnectError::Connector(ironrdp_connector::custom_err!("gravar known_hosts", std::io::Error::other(e.to_string()))))?;

    Ok(())
}

pub(crate) async fn connect(
    profile: &ConnectionProfile,
    username: &str,
    password: &str,
    cliprdr: ironrdp_cliprdr::Cliprdr<ironrdp_cliprdr::Client>,
    events: &mpsc::Sender<SessionEvent>,
) -> Result<Connected, ConnectError> {
    let address = profile.address();

    let stream = TcpStream::connect(&address).await?;
    stream.set_nodelay(true)?;
    let client_addr = stream.local_addr()?;

    let mut framed: TokioFramed<TcpStream> = TokioFramed::new(stream);

    let config = build_config(profile, username, password);
    let mut connector = ClientConnector::new(config, client_addr);
    connector.attach_static_channel(cliprdr);

    let should_upgrade = ironrdp_tokio::connect_begin(&mut framed, &mut connector).await?;

    let (initial_stream, leftover) = framed.into_inner();
    let (tls_stream, tls_cert) = ironrdp_tls::upgrade(initial_stream, &profile.host).await?;

    confirm_certificate(&address, &tls_cert, events).await?;

    let upgraded = ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);

    let mut framed: Framed<ironrdp_tokio::TokioStream<TlsStream<TcpStream>>> =
        Framed::new_with_leftover(tls_stream, leftover);

    let server_public_key = ironrdp_tls::extract_tls_server_public_key(&tls_cert)
        .ok_or_else(|| ironrdp_connector::general_err!("chave pública do servidor ausente no certificado TLS"))?
        .to_owned();

    let connection_result = ironrdp_tokio::connect_finalize(
        upgraded,
        connector,
        &mut framed,
        &mut NoNetworkClient,
        ServerName::new(&profile.host),
        server_public_key,
        None,
    )
    .await?;

    Ok(Connected {
        connection_result,
        framed,
    })
}
