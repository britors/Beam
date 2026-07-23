//! TOFU certificate confirmation dialog: first-use confirmation, and a loud SSH-style warning
//! when a previously trusted fingerprint changes.

use adw::prelude::*;
use beam_core::known_hosts::TrustDecision;

pub async fn confirm(parent: &impl IsA<gtk::Widget>, address: &str, fingerprint: &str, decision: &TrustDecision) -> bool {
    let (heading, body, response_label, is_destructive) = match decision {
        TrustDecision::FirstUse => (
            "Verificar identidade do servidor".to_string(),
            format!(
                "Esta é a primeira conexão com <b>{address}</b>.\n\n\
                 Impressão digital do certificado (SHA-256):\n<tt>{fingerprint}</tt>\n\n\
                 Confirme com o administrador do servidor que esta impressão digital está correta \
                 antes de continuar.",
            ),
            "Confiar e conectar",
            false,
        ),
        TrustDecision::Mismatch { previous } => (
            "⚠ Possível ataque detectado".to_string(),
            format!(
                "O certificado apresentado por <b>{address}</b> mudou desde a última conexão.\n\n\
                 Impressão digital anterior:\n<tt>{previous}</tt>\n\n\
                 Impressão digital atual:\n<tt>{fingerprint}</tt>\n\n\
                 Isso pode indicar uma tentativa de interceptação da conexão (man-in-the-middle), \
                 ou apenas que o certificado do servidor foi renovado. Só continue se tiver certeza \
                 da causa.",
            ),
            "Confiar mesmo assim",
            true,
        ),
        TrustDecision::Trusted => return true,
    };

    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .body_use_markup(true)
        .build();
    dialog.add_response("cancel", "Cancelar");
    dialog.add_response("trust", response_label);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    if is_destructive {
        dialog.set_response_appearance("trust", adw::ResponseAppearance::Destructive);
    } else {
        dialog.set_response_appearance("trust", adw::ResponseAppearance::Suggested);
    }

    let response = dialog.choose_future(Some(parent)).await;
    response == "trust"
}
