//! Password prompt shown when a session needs credentials that weren't found in the keyring.

use adw::prelude::*;

/// Returns `Some((password, save_to_keyring))`, or `None` if the user cancelled.
pub async fn ask(parent: &impl IsA<gtk::Widget>, username: &str) -> Option<(String, bool)> {
    let password_row = adw::PasswordEntryRow::builder().title("Senha").build();

    let save_row = adw::SwitchRow::builder()
        .title("Salvar no chaveiro")
        .subtitle("Usar o Serviço de Segredos do sistema para não pedir novamente")
        .active(true)
        .build();

    let group = adw::PreferencesGroup::new();
    group.add(&password_row);
    group.add(&save_row);

    let dialog = adw::AlertDialog::builder()
        .heading("Autenticação necessária")
        .body(format!("Digite a senha da conta “{username}” para continuar."))
        .extra_child(&group)
        .build();
    dialog.add_response("cancel", "Cancelar");
    dialog.add_response("connect", "Conectar");
    dialog.set_default_response(Some("connect"));
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("connect", adw::ResponseAppearance::Suggested);
    dialog.set_response_enabled("connect", false);

    password_row.connect_changed({
        let dialog = dialog.clone();
        move |entry| {
            dialog.set_response_enabled("connect", !entry.text().is_empty());
        }
    });

    let response = dialog.choose_future(Some(parent)).await;
    if response != "connect" {
        return None;
    }

    let password = password_row.text().to_string();
    if password.is_empty() {
        return None;
    }
    Some((password, save_row.is_active()))
}
