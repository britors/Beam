//! Main window: searchable list of saved connection profiles.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::ObjectSubclassIsExt;
use beam_core::profile::{self, ConnectionProfile};
use gtk::gio;
use gtk::glib;
use gtk::glib::clone;

use crate::{profile_dialog, settings, window_session};

pub fn build(app: &adw::Application, runtime: tokio::runtime::Handle) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Beam")
        .default_width(560)
        .default_height(640)
        .build();

    let profiles: Rc<RefCell<Vec<ConnectionProfile>>> = Rc::new(RefCell::new(profile::load_profiles().unwrap_or_default()));

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let add_btn = gtk::Button::from_icon_name("list-add-symbolic");
    add_btn.set_tooltip_text(Some("Nova conexão"));
    header.pack_start(&add_btn);

    let menu = gio::Menu::new();
    let settings_section = gio::Menu::new();
    settings_section.append(Some("Configurações"), Some("win.settings"));
    menu.append_section(None, &settings_section);
    let about_section = gio::Menu::new();
    about_section.append(Some("Sobre o Beam"), Some("win.about"));
    menu.append_section(None, &about_section);
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Menu principal")
        .menu_model(&menu)
        .build();
    header.pack_end(&menu_button);

    toolbar_view.add_top_bar(&header);

    install_window_actions(&window);

    let search_entry = gtk::SearchEntry::builder().margin_start(12).margin_end(12).margin_top(6).build();

    let list_store = gio::ListStore::new::<ConnectionProfileObject>();
    for p in profiles.borrow().iter() {
        list_store.append(&ConnectionProfileObject::new(p.clone()));
    }

    let filter = gtk::CustomFilter::new(clone!(
        #[weak]
        search_entry,
        #[upgrade_or]
        true,
        move |item| {
            let query = search_entry.text().to_lowercase();
            if query.is_empty() {
                return true;
            }
            let obj = item.downcast_ref::<ConnectionProfileObject>().expect("ConnectionProfileObject");
            let p = obj.profile();
            p.name.to_lowercase().contains(&query) || p.host.to_lowercase().contains(&query)
        }
    ));
    let filter_model = gtk::FilterListModel::new(Some(list_store.clone()), Some(filter.clone()));
    search_entry.connect_search_changed(move |_| filter.changed(gtk::FilterChange::Different));

    let selection_model = gtk::NoSelection::new(Some(filter_model));

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, list_item| {
        let row = adw::ActionRow::new();
        let connect_icon = gtk::Image::from_icon_name("network-server-symbolic");
        row.add_prefix(&connect_icon);

        let menu_btn = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        row.add_suffix(&menu_btn);
        row.set_activatable(true);

        list_item
            .downcast_ref::<gtk::ListItem>()
            .expect("ListItem")
            .set_child(Some(&row));
    });

    factory.connect_bind(clone!(
        #[strong]
        profiles,
        #[strong]
        list_store,
        #[weak]
        window,
        #[strong]
        runtime,
        move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().expect("ListItem");
            let obj = list_item.item().and_downcast::<ConnectionProfileObject>().expect("item");
            let row = list_item.child().and_downcast::<adw::ActionRow>().expect("row");
            let p = obj.profile();
            row.set_title(&glib::markup_escape_text(&p.name));
            row.set_subtitle(&glib::markup_escape_text(&format!("{}@{}", p.username, p.address())));

            let menu_btn = row
                .last_child()
                .and_then(|w| w.prev_sibling())
                .and_downcast::<gtk::MenuButton>();
            if let Some(menu_btn) = menu_btn {
                let menu = gio::Menu::new();
                menu.append(Some("Editar"), Some("row.edit"));
                menu.append(Some("Duplicar"), Some("row.duplicate"));
                menu.append(Some("Excluir"), Some("row.delete"));
                let popover = gtk::PopoverMenu::from_model(Some(&menu));
                menu_btn.set_popover(Some(&popover));

                let actions = gio::SimpleActionGroup::new();
                let edit_action = gio::SimpleAction::new("edit", None);
                edit_action.connect_activate(clone!(
                    #[strong]
                    profiles,
                    #[strong]
                    list_store,
                    #[weak]
                    window,
                    #[strong]
                    obj,
                    move |_, _| {
                        let profiles = profiles.clone();
                        let list_store = list_store.clone();
                        let window = window.clone();
                        let current = obj.profile();
                        glib::MainContext::default().spawn_local(async move {
                            if let Some(updated) = profile_dialog::edit(&window, Some(current)).await {
                                let mut list = profiles.borrow_mut();
                                if let Some(slot) = list.iter_mut().find(|p| p.id == updated.id) {
                                    *slot = updated;
                                }
                                let _ = profile::save_profiles(&list);
                                drop(list);
                                refresh_store(&profiles, &list_store);
                            }
                        });
                    }
                ));
                let duplicate_action = gio::SimpleAction::new("duplicate", None);
                duplicate_action.connect_activate(clone!(
                    #[strong]
                    profiles,
                    #[strong]
                    list_store,
                    #[strong]
                    obj,
                    move |_, _| {
                        let mut list = profiles.borrow_mut();
                        list.push(obj.profile().duplicate());
                        let _ = profile::save_profiles(&list);
                        drop(list);
                        refresh_store(&profiles, &list_store);
                    }
                ));
                let delete_action = gio::SimpleAction::new("delete", None);
                delete_action.connect_activate(clone!(
                    #[strong]
                    profiles,
                    #[strong]
                    list_store,
                    #[strong]
                    obj,
                    move |_, _| {
                        let target = obj.profile();
                        let target_for_secret = target.clone();
                        glib::MainContext::default().spawn_local(async move {
                            let key = beam_core::secrets::SecretKey {
                                host: &target_for_secret.host,
                                port: target_for_secret.port,
                                user: &target_for_secret.username,
                            };
                            let _ = beam_core::secrets::delete_password(&key).await;
                        });
                        let mut list = profiles.borrow_mut();
                        list.retain(|p| p.id != target.id);
                        let _ = profile::save_profiles(&list);
                        drop(list);
                        refresh_store(&profiles, &list_store);
                    }
                ));
                actions.add_action(&edit_action);
                actions.add_action(&duplicate_action);
                actions.add_action(&delete_action);
                row.insert_action_group("row", Some(&actions));
            }

            row.connect_activated(clone!(
                #[weak]
                window,
                #[strong]
                runtime,
                #[strong]
                obj,
                move |_| {
                    window_session::open(
                        window.application().and_downcast::<adw::Application>().as_ref().expect("app"),
                        obj.profile(),
                        runtime.clone(),
                    );
                }
            ));
        }
    ));

    let list_view = gtk::ListView::new(Some(selection_model), Some(factory));
    list_view.set_single_click_activate(true);

    let scroller = gtk::ScrolledWindow::builder().child(&list_view).vexpand(true).build();

    let status_page = adw::StatusPage::builder()
        .title("Nenhuma conexão")
        .description("Crie a primeira conexão para começar")
        .icon_name("network-server-symbolic")
        .vexpand(true)
        .build();

    let stack = gtk::Stack::new();
    stack.add_named(&status_page, Some("empty"));
    stack.add_named(&scroller, Some("list"));
    update_stack(&stack, &list_store);
    list_store.connect_items_changed(clone!(
        #[weak]
        stack,
        move |store, _, _, _| update_stack(&stack, store)
    ));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&search_entry);
    content.append(&stack);
    toolbar_view.set_content(Some(&content));
    window.set_content(Some(&toolbar_view));

    add_btn.connect_clicked(clone!(
        #[weak]
        window,
        #[strong]
        profiles,
        #[strong]
        list_store,
        #[strong]
        runtime,
        move |_| {
            let profiles = profiles.clone();
            let list_store = list_store.clone();
            let window = window.clone();
            let _ = &runtime;
            glib::MainContext::default().spawn_local(async move {
                if let Some(new_profile) = profile_dialog::edit(&window, None).await {
                    let mut list = profiles.borrow_mut();
                    list.push(new_profile);
                    let _ = profile::save_profiles(&list);
                    drop(list);
                    refresh_store(&profiles, &list_store);
                }
            });
        }
    ));

    window.present();
}

fn install_window_actions(window: &adw::ApplicationWindow) {
    let settings_action = gio::SimpleAction::new("settings", None);
    settings_action.connect_activate(clone!(
        #[weak]
        window,
        move |_, _| settings::show(&window)
    ));
    window.add_action(&settings_action);

    let about_action = gio::SimpleAction::new("about", None);
    about_action.connect_activate(clone!(
        #[weak]
        window,
        move |_, _| {
            let dialog = adw::AboutDialog::builder()
                .application_name("Beam")
                .application_icon("org.lyraos.Beam")
                .developer_name("Lyra Linux")
                .version(env!("CARGO_PKG_VERSION"))
                .website("https://github.com/britors/Beam")
                .issue_url("https://github.com/britors/Beam/issues")
                .license_type(gtk::License::Gpl30)
                .build();
            dialog.set_developers(&["Rodrigo Brito"]);
            dialog.present(Some(&window));
        }
    ));
    window.add_action(&about_action);
}

fn update_stack(stack: &gtk::Stack, store: &gio::ListStore) {
    stack.set_visible_child_name(if store.n_items() == 0 { "empty" } else { "list" });
}

fn refresh_store(profiles: &Rc<RefCell<Vec<ConnectionProfile>>>, store: &gio::ListStore) {
    store.remove_all();
    for p in profiles.borrow().iter() {
        store.append(&ConnectionProfileObject::new(p.clone()));
    }
}

glib::wrapper! {
    pub struct ConnectionProfileObject(ObjectSubclass<imp::ConnectionProfileObject>);
}

impl ConnectionProfileObject {
    pub fn new(profile: ConnectionProfile) -> Self {
        let obj: Self = glib::Object::new();
        obj.imp().profile.replace(Some(profile));
        obj
    }

    pub fn profile(&self) -> ConnectionProfile {
        self.imp().profile.borrow().clone().expect("profile set at construction")
    }
}

mod imp {
    use std::cell::RefCell;

    use beam_core::profile::ConnectionProfile;
    use gtk::glib;
    use gtk::subclass::prelude::*;

    #[derive(Default)]
    pub struct ConnectionProfileObject {
        pub profile: RefCell<Option<ConnectionProfile>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ConnectionProfileObject {
        const NAME: &'static str = "BeamConnectionProfileObject";
        type Type = super::ConnectionProfileObject;
    }

    impl ObjectImpl for ConnectionProfileObject {}
}
