//! Application settings dialog. Beam has no global preferences yet (connection settings
//! live per-profile in `profile_dialog`), so this currently only manages the TOFU known-hosts
//! store — the one piece of global state `beam-core` keeps outside of profiles.

use std::cell::RefCell;
use std::rc::Rc;

use crate::i18n::gettext;
use adw::prelude::*;
use gtk::glib;

pub fn show(parent: &adw::ApplicationWindow) {
    let dialog = adw::PreferencesDialog::builder()
        .title(gettext("Settings"))
        .content_width(520)
        .content_height(480)
        .build();

    let page = adw::PreferencesPage::builder()
        .title(gettext("Security"))
        .icon_name("security-high-symbolic")
        .build();

    let group = adw::PreferencesGroup::builder()
        .title(gettext("Trusted certificates"))
        .description(gettext("Certificate fingerprints confirmed on previous connections (TOFU verification). Removing an entry requires confirmation again the next time you connect to that server."))
        .build();

    let rows: Rc<RefCell<Vec<gtk::Widget>>> = Rc::new(RefCell::new(Vec::new()));
    refresh_known_hosts(&group, &rows);

    page.add(&group);
    dialog.add(&page);
    dialog.present(Some(parent));
}

fn refresh_known_hosts(group: &adw::PreferencesGroup, rows: &Rc<RefCell<Vec<gtk::Widget>>>) {
    for widget in rows.borrow_mut().drain(..) {
        group.remove(&widget);
    }

    let hosts = beam_core::known_hosts::list().unwrap_or_default();
    if hosts.is_empty() {
        let row = adw::ActionRow::builder()
            .title(gettext("No trusted certificates yet"))
            .sensitive(false)
            .build();
        group.add(&row);
        rows.borrow_mut().push(row.upcast());
        return;
    }

    for (address, fingerprint) in hosts {
        let row = adw::ActionRow::new();
        row.set_title(&glib::markup_escape_text(&address));
        row.set_subtitle(&glib::markup_escape_text(fingerprint.as_str()));

        let remove_btn = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .tooltip_text(gettext("Remove trust"))
            .build();
        remove_btn.update_property(&[gtk::accessible::Property::Label(&gettext("Remove trust"))]);
        row.add_suffix(&remove_btn);

        let group_for_remove = group.clone();
        let rows_for_remove = rows.clone();
        remove_btn.connect_clicked(move |_| {
            let _ = beam_core::known_hosts::remove(&address);
            refresh_known_hosts(&group_for_remove, &rows_for_remove);
        });

        group.add(&row);
        rows.borrow_mut().push(row.upcast());
    }
}
