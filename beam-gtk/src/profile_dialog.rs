//! Create/edit dialog for a [`ConnectionProfile`].

use crate::i18n::gettext;
use adw::prelude::*;
use beam_core::profile::{ConnectionProfile, Resolution};
use gtk::glib;

/// Show the profile editor. `initial` is `None` when creating a new profile, `Some(profile)`
/// when editing an existing one. Returns the saved profile, or `None` if cancelled.
pub async fn edit(
    parent: &impl IsA<gtk::Widget>,
    initial: Option<ConnectionProfile>,
) -> Option<ConnectionProfile> {
    let is_new = initial.is_none();
    let base = initial.unwrap_or_else(|| ConnectionProfile::new("", "", ""));

    let name_row = adw::EntryRow::builder()
        .title(gettext("Connection name"))
        .text(&base.name)
        .build();
    let host_row = adw::EntryRow::builder()
        .title(gettext("Server (host)"))
        .text(&base.host)
        .build();
    let port_row = adw::SpinRow::builder()
        .title(gettext("Port"))
        .adjustment(&gtk::Adjustment::new(
            f64::from(base.port),
            1.0,
            65535.0,
            1.0,
            10.0,
            0.0,
        ))
        .build();
    let user_row = adw::EntryRow::builder()
        .title(gettext("Username"))
        .text(&base.username)
        .build();
    let domain_row = adw::EntryRow::builder()
        .title(gettext("Domain (optional)"))
        .text(base.domain.clone().unwrap_or_default())
        .build();

    let resolution_row = adw::ComboRow::builder()
        .title(gettext("Resolution"))
        .build();
    let resolution_labels: Vec<String> = Resolution::PRESETS
        .iter()
        .map(|r| format!("{}×{}", r.width, r.height))
        .chain(std::iter::once(gettext("Custom…")))
        .collect();
    let resolution_model = gtk::StringList::new(
        &resolution_labels
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    resolution_row.set_model(Some(&resolution_model));
    let preset_index = Resolution::PRESETS
        .iter()
        .position(|r| *r == base.resolution);
    resolution_row.set_selected(preset_index.unwrap_or(Resolution::PRESETS.len()) as u32);

    let custom_width_row = adw::SpinRow::builder()
        .title(gettext("Custom width"))
        .adjustment(&gtk::Adjustment::new(
            f64::from(base.resolution.width),
            320.0,
            7680.0,
            1.0,
            10.0,
            0.0,
        ))
        .visible(preset_index.is_none())
        .build();
    let custom_height_row = adw::SpinRow::builder()
        .title(gettext("Custom height"))
        .adjustment(&gtk::Adjustment::new(
            f64::from(base.resolution.height),
            240.0,
            4320.0,
            1.0,
            10.0,
            0.0,
        ))
        .visible(preset_index.is_none())
        .build();

    resolution_row.connect_selected_notify(gtk::glib::clone!(
        #[weak]
        custom_width_row,
        #[weak]
        custom_height_row,
        move |row| {
            let is_custom = row.selected() as usize == Resolution::PRESETS.len();
            custom_width_row.set_visible(is_custom);
            custom_height_row.set_visible(is_custom);
        }
    ));

    let depth_row = adw::ComboRow::builder()
        .title(gettext("Color depth"))
        .build();
    let depth_model = gtk::StringList::new(&[&gettext("16 bits"), &gettext("32 bits")]);
    depth_row.set_model(Some(&depth_model));
    depth_row.set_selected(if base.color_depth == 16 { 0 } else { 1 });

    let fullscreen_row = adw::SwitchRow::builder()
        .title(gettext("Open in fullscreen"))
        .active(base.fullscreen)
        .build();

    let connection_group = adw::PreferencesGroup::builder()
        .title(gettext("Connection"))
        .build();
    connection_group.add(&name_row);
    connection_group.add(&host_row);
    connection_group.add(&port_row);

    let auth_group = adw::PreferencesGroup::builder()
        .title(gettext("Authentication"))
        .build();
    auth_group.add(&user_row);
    auth_group.add(&domain_row);

    let display_group = adw::PreferencesGroup::builder()
        .title(gettext("Display"))
        .build();
    display_group.add(&resolution_row);
    display_group.add(&custom_width_row);
    display_group.add(&custom_height_row);
    display_group.add(&depth_row);
    display_group.add(&fullscreen_row);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&connection_group);
    content.append(&auth_group);
    content.append(&display_group);

    let scroller = gtk::ScrolledWindow::builder()
        .child(&content)
        .propagate_natural_height(true)
        .min_content_width(420)
        .build();

    let dialog = adw::Dialog::builder()
        .title(if is_new {
            gettext("New connection")
        } else {
            gettext("Edit connection")
        })
        .content_width(460)
        .content_height(560)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let save_btn = gtk::Button::builder()
        .label(gettext("Save"))
        .css_classes(["suggested-action"])
        .build();
    let cancel_btn = gtk::Button::builder().label(gettext("Cancel")).build();
    header.pack_start(&cancel_btn);
    header.pack_end(&save_btn);
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&scroller));
    dialog.set_child(Some(&toolbar_view));

    save_btn.set_sensitive(!base.host.is_empty());
    let update_sensitivity = gtk::glib::clone!(
        #[weak]
        name_row,
        #[weak]
        host_row,
        #[weak]
        save_btn,
        move || {
            save_btn.set_sensitive(!host_row.text().is_empty() && !name_row.text().is_empty());
        }
    );
    name_row.connect_changed(gtk::glib::clone!(
        #[strong]
        update_sensitivity,
        move |_| update_sensitivity()
    ));
    host_row.connect_changed(move |_| update_sensitivity());

    let result: std::rc::Rc<std::cell::RefCell<Option<ConnectionProfile>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));

    cancel_btn.connect_clicked(gtk::glib::clone!(
        #[weak]
        dialog,
        move |_| {
            dialog.close();
        }
    ));

    save_btn.connect_clicked(gtk::glib::clone!(
        #[weak]
        dialog,
        #[strong]
        result,
        #[strong]
        base,
        move |_| {
            let resolution = if (resolution_row.selected() as usize) < Resolution::PRESETS.len() {
                Resolution::PRESETS[resolution_row.selected() as usize]
            } else {
                Resolution {
                    width: custom_width_row.value() as u16,
                    height: custom_height_row.value() as u16,
                }
            };

            let mut profile = base.clone();
            profile.name = name_row.text().to_string();
            profile.host = host_row.text().to_string();
            profile.port = port_row.value() as u16;
            profile.username = user_row.text().to_string();
            profile.domain = {
                let d = domain_row.text().to_string();
                (!d.is_empty()).then_some(d)
            };
            profile.resolution = resolution;
            profile.color_depth = if depth_row.selected() == 0 { 16 } else { 32 };
            profile.fullscreen = fullscreen_row.is_active();

            *result.borrow_mut() = Some(profile);
            dialog.close();
        }
    ));

    dialog.present(Some(parent));

    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = std::cell::RefCell::new(Some(tx));
    dialog.connect_closed(gtk::glib::clone!(
        #[strong]
        result,
        move |_| {
            if let Some(tx) = tx.borrow_mut().take() {
                let _ = tx.send(result.borrow_mut().take());
            }
        }
    ));

    rx.await.ok().flatten()
}
