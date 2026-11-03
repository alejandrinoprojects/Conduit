pub mod launcher;
pub mod mods;
pub mod process;
pub mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            launcher::login_microsoft,
            launcher::load_saved_auth,
            launcher::install_instance,
            mods::search_modrinth,
            mods::install_modrinth_mod,
            mods::list_installed_mods,
            mods::delete_mod,
            process::start_server,
            process::stop_server,
            process::launch_client,
            process::join_local_server,
            process::send_server_command,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
