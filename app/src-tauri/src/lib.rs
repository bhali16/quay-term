use quay_core::AppInfo;

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo::current(env!("CARGO_PKG_VERSION"))
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_app_info])
        .run(tauri::generate_context!())
        .expect("error while running quay-term");
}
