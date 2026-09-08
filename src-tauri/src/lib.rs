mod discovery;
mod server;
mod state;
mod transfer;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "info,lanlink=info,lanlink_lib=info".into()
            }),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(server::spawn(handle)).map_err(|err| {
                tracing::error!("failed to start LAN server: {err}");
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    err,
                )) as Box<dyn std::error::Error>
            })?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Lanlink");
}
