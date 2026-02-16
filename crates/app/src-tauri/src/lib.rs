// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_app_info() -> serde_json::Value {
    serde_json::json!({
        "backend_url": "http://127.0.0.1:3000",
        "version": env!("CARGO_PKG_VERSION")
    })
}

mod audit_log;
mod backend;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|_app| {
            tauri::async_runtime::spawn(async move {
                if let Err(e) = backend::start_server().await {
                    eprintln!("Failed to start backend server: {}", e);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, get_app_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
