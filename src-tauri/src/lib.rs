pub mod credential;
pub mod provider;
pub mod refresher;
pub mod state;
pub mod tray;

use state::AppState;

#[tauri::command]
async fn get_usage_state(state: tauri::State<'_, AppState>) -> Result<state::UsageState, String> {
    Ok(state.snapshot().await)
}

#[tauri::command]
async fn refresh_now(state: tauri::State<'_, AppState>) -> Result<state::UsageState, String> {
    state.refresh_now().await.map_err(|e| e.to_string())?;
    Ok(state.snapshot().await)
}

#[tauri::command]
async fn open_login_window(app: tauri::AppHandle) -> Result<(), String> {
    tray::open_login_window(&app).map_err(|e| e.to_string())
}

#[tauri::command]
async fn logout(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.logout().await.map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            get_usage_state,
            refresh_now,
            open_login_window,
            logout,
            tray::credential_candidate
        ])
        .setup(|app| {
            tray::setup_tray(app.handle())?;
            refresher::spawn_refresher(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "panel" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
