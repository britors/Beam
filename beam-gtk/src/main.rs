mod cert_dialog;
mod display;
mod input_gtk;
mod password_dialog;
mod profile_dialog;
mod settings;
mod window_manager;
mod window_session;

use gtk::glib;
use gtk::prelude::*;

const APP_ID: &str = "org.lyraos.Beam";

fn main() -> glib::ExitCode {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_env_var("BEAM_LOG")
        .with_default_directive(tracing::level_filters::LevelFilter::WARN.into())
        .from_env_lossy();
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // The tokio runtime drives every RDP session on its own thread; the GTK main loop stays on
    // this (the main) thread. Frontend code hands events between the two only via the channels
    // exposed by `beam_core::session` (never by blocking either loop on the other).
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to start the tokio runtime");
    let runtime_handle = runtime.handle().clone();
    std::thread::Builder::new()
        .name("beam-tokio".to_owned())
        .spawn(move || {
            runtime.block_on(std::future::pending::<()>());
        })
        .expect("failed to spawn the tokio runtime thread");

    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(move |app| {
        window_manager::build(app, runtime_handle.clone());
    });
    app.run()
}
