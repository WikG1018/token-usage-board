use crate::credential::Credential;
use crate::state::AppState;
use tauri::image::Image;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

const LOGIN_URL: &str = "https://platform.xiaomimimo.com/console/plan-manage";

pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItemBuilder::with_id("show", "打开面板").build(app)?;
    let login = MenuItemBuilder::with_id("login", "重新登录").build(app)?;
    let refresh = MenuItemBuilder::with_id("refresh", "刷新").build(app)?;
    let logout = MenuItemBuilder::with_id("logout", "断开连接").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&show, &login, &refresh, &logout, &quit])
        .build()?;

    let icon = default_icon();
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .tooltip("Token Usage Board")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => position_and_show_panel(app),
            "login" => {
                let _ = open_login_window(app);
            }
            "refresh" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    let _ = state.refresh_now().await;
                });
            }
            "logout" => {
                let app_clone = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app_clone.state::<AppState>();
                    let _ = state.logout().await;
                });
                hide_panel(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Enter { .. } = event {
                position_and_show_panel(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn update_tray_tooltip(app: &AppHandle, tip: &str) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_tooltip(Some(tip));
    }
}

fn default_icon() -> Image<'static> {
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let c = (size / 2) as f32;
    let r = (size / 2 - 2) as f32;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - c + 0.5;
            let dy = y as f32 - c + 0.5;
            let inside = (dx * dx + dy * dy).sqrt() <= r;
            let i = ((y * size + x) * 4) as usize;
            if inside {
                rgba[i] = 79;
                rgba[i + 1] = 140;
                rgba[i + 2] = 255;
                rgba[i + 3] = 255;
            } else {
                rgba[i + 3] = 0;
            }
        }
    }
    Image::new_owned(rgba, size, size)
}

fn position_and_show_panel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        if let Ok(monitor) = w.current_monitor() {
            if let Some(monitor) = monitor {
                let screen = monitor.size();
                let pos = monitor.position();
                let scale = monitor.scale_factor();
                // 面板逻辑尺寸 340x420，贴近屏幕右下角托盘区上方
                let logical_w = 360.0;
                let logical_h = 440.0;
                let x = pos.x as f64 + screen.width as f64 / scale - logical_w - 12.0;
                let y = pos.y as f64 + screen.height as f64 / scale - logical_h - 12.0;
                let _ = w.set_position(PhysicalPosition::new(
                    (x * scale) as i32,
                    (y * scale) as i32,
                ));
            }
        }
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn hide_panel(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.hide();
    }
}

pub fn open_login_window(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(w) = app.get_webview_window("login") {
        let _ = w.close();
    }
    WebviewWindowBuilder::new(app, "login", WebviewUrl::External(LOGIN_URL.parse()?))
        .title("连接 Xiaomi MiMo")
        .inner_size(480.0, 720.0)
        .resizable(true)
        .initialization_script(CAPTURE_SCRIPT)
        .build()?;
    Ok(())
}

const CAPTURE_SCRIPT: &str = r#"
(function () {
  function parseCookies(cookieStr) {
    const out = [];
    if (!cookieStr) return out;
    cookieStr.split(/;\s*/).forEach(function (pair) {
      if (!pair) return;
      const idx = pair.indexOf("=");
      if (idx < 0) {
        out.push([pair.trim(), ""]);
      } else {
        out.push([pair.slice(0, idx).trim(), pair.slice(idx + 1).trim()]);
      }
    });
    return out;
  }
  const origFetch = window.fetch;
  window.fetch = async function (...args) {
    try {
      const [input, init] = args;
      const url = typeof input === "string" ? input : (input && input.url);
      if (url && /usage|plan|credit|quota/i.test(url)) {
        const headers = (init && init.headers) || {};
        const cookies = parseCookies(document.cookie || "");
        window.__TAURI_INTERNALS__?.invoke("credential_candidate", {
          endpoint: url,
          headers: headers,
          cookies: cookies,
        });
      }
    } catch (e) {}
    return origFetch.apply(this, args);
  };
})();
"#;

#[tauri::command]
pub async fn credential_candidate(
    app: tauri::AppHandle,
    endpoint: String,
    headers: serde_json::Value,
    cookies: Vec<(String, String)>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let cred = Credential {
        endpoint,
        cookies,
        extra_headers: serde_json::from_value::<Vec<(String, String)>>(headers)
            .unwrap_or_default(),
        obtained_at: chrono::Utc::now().timestamp(),
    };
    state
        .on_credential_captured(cred)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window("login") {
        let _ = w.close();
    }
    Ok(())
}
