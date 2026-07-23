use crate::state::{tooltip_for, AppState};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub fn spawn_refresher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            let _ = state.refresh_now().await;
            let snap = state.snapshot().await;
            crate::tray::update_tray_tooltip(&app, &tooltip_for(&snap));
            let _ = app.emit("usage-updated", snap);
            let wait = state.backoff_secs().await;
            tokio::time::sleep(Duration::from_secs(wait)).await;
        }
    });
}
