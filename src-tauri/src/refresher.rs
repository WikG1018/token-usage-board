use crate::state::AppState;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub fn spawn_refresher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            let _ = state.refresh_now().await;
            let _ = app.emit("usage-updated", state.snapshot().await);
            let wait = state.backoff_secs().await;
            tokio::time::sleep(Duration::from_secs(wait)).await;
        }
    });
}
