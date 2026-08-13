//! TOFU certificate confirmation dialog: first-use confirmation, and a loud SSH-style warning
//! when a previously trusted fingerprint changes.

use crate::i18n::gettext;
use adw::prelude::*;
use beam_core::known_hosts::TrustDecision;

pub async fn confirm(
    parent: &impl IsA<gtk::Widget>,
    address: &str,
    fingerprint: &str,
    decision: &TrustDecision,
) -> bool {
    let (heading, body, response_label, is_destructive) = match decision {
        TrustDecision::FirstUse => (
            gettext("Verify server identity"),
            gettext("This is the first connection to <b>{address}</b>.\n\nCertificate fingerprint (SHA-256):\n<tt>{fingerprint}</tt>\n\nConfirm with the server administrator that this fingerprint is correct before continuing.")
                .replace("{address}", address)
                .replace("{fingerprint}", fingerprint),
            gettext("Trust and connect"),
            false,
        ),
        TrustDecision::Mismatch { previous } => (
            gettext("⚠ Possible attack detected"),
            gettext("The certificate presented by <b>{address}</b> has changed since the last connection.\n\nPrevious fingerprint:\n<tt>{previous}</tt>\n\nCurrent fingerprint:\n<tt>{fingerprint}</tt>\n\nThis may indicate an attempt to intercept the connection (man-in-the-middle), or simply that the server certificate was renewed. Continue only if you are certain of the reason.")
                .replace("{address}", address)
                .replace("{previous}", previous.as_str())
                .replace("{fingerprint}", fingerprint),
            gettext("Trust anyway"),
            true,
        ),
        TrustDecision::Trusted => return true,
    };

    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .body_use_markup(true)
        .build();
    dialog.add_response("cancel", &gettext("Cancel"));
    dialog.add_response("trust", &response_label);
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
