use anyhow::{Context, Result};
use std::sync::Arc;
use taskhub_core::{daemon::{run_daemon, DaemonConfig}, Engine, Storage};
use taskhub_wasm_host::PluginRegistry;

use crate::commands::init::{db_path, taskhub_dir};

pub async fn run(tray: bool) -> Result<()> {
    let dir = taskhub_dir()?;
    let db = db_path()?;
    if !db.exists() {
        anyhow::bail!("taskhub not initialized — run `taskhub init` first");
    }

    let storage = Arc::new(Storage::open(&db).context("open storage")?);
    let mut engine = Engine::new(storage.clone());
    taskhub_plugins_core::register_all(&mut engine);

    let registry = PluginRegistry::new(dir.join("plugins"));
    if let Err(e) = registry.register_all(&mut engine) {
        tracing::warn!("some plugins failed to load: {e}");
    }

    let engine = Arc::new(engine);
    let home = dirs::home_dir().context("no home dir")?;
    let config = DaemonConfig::default_for_home(&home);

    if tray {
        std::thread::spawn(start_tray);
    }

    run_daemon(config, engine, storage).await
}

fn start_tray() {
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        use tray_icon::{menu::{Menu, MenuItem}, TrayIconBuilder};

        let menu = Menu::new();
        let quit = MenuItem::new("Quit TaskHub", true, None);
        let _ = menu.append(&quit);

        let _tray = TrayIconBuilder::new()
            .with_tooltip("TaskHub")
            .with_menu(Box::new(menu))
            .build();

        #[cfg(target_os = "windows")]
        pump_windows_messages();
    }
}

#[cfg(target_os = "windows")]
fn pump_windows_messages() {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
