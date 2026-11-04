pub mod gateway;
pub mod launcher;
pub mod mods;
pub mod network;
pub mod process;
pub mod state;

use state::{AppState, ServerStatus};
use tauri::Manager;
use tokio::io::AsyncWriteExt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                // Ensure Minecraft server and playit tunnels are cleanly shutdown before app closes
                let state = window.state::<AppState>();
                let state_clone = state.inner().clone();
                tauri::async_runtime::block_on(async move {
                    let mut guard = state_clone.lock().await;
                    
                    // Gracefully send "stop" to server stdin if running
                    if let Some(mut stdin) = guard.server_stdin.take() {
                        let _ = stdin.write_all(b"stop\n").await;
                        let _ = stdin.flush().await;
                    }

                    // If server is still in starting/running mode, terminate cleanly
                    if let Some(mut child) = guard.server_child.take() {
                        let wait_res = tokio::time::timeout(tokio::time::Duration::from_secs(4), child.wait()).await;
                        if wait_res.is_err() {
                            let _ = child.kill().await;
                        }
                    }

                    // Kill tunnel process
                    if let Some(mut tunnel) = guard.tunnel_child.take() {
                        let _ = tunnel.kill().await;
                    }

                    // Stop on-demand gateway if active
                    let _ = gateway::stop_gateway().await;

                    guard.server_status = ServerStatus::Stopped;
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            launcher::login_microsoft,
            launcher::load_saved_auth,
            launcher::list_all_minecraft_versions,
            launcher::install_instance,
            launcher::detect_installed_javas,
            mods::search_modrinth,
            mods::get_mod_versions,
            mods::get_modrinth_project_detail,
            mods::switch_mod_version,
            mods::install_modrinth_mod,
            mods::install_modrinth_project,
            mods::list_installed_mods,
            mods::toggle_mod,
            mods::delete_mod,
            mods::delete_player_data,
            mods::list_worlds,
            mods::import_external_saves,
            mods::list_downloaded_runtimes,
            mods::delete_downloaded_runtime,
            mods::delete_realm,
            mods::delete_instance,
            mods::get_realm_prune_info,
            mods::execute_realm_prune,
            mods::create_new_realm,
            mods::create_server_wizard,
            mods::get_deduplication_metrics,
            mods::verify_and_repair_junctions,
            process::start_server,
            process::stop_server,
            process::launch_client,
            process::join_local_server,
            process::open_instance_folder,
            process::send_server_command,
            gateway::get_gateway_config,
            gateway::save_gateway_config,
            gateway::start_gateway,
            gateway::stop_gateway,
            gateway::get_gateway_state,
            network::conduit_get_identity,
            network::conduit_host_realm,
            network::conduit_stop_hosting,
            network::conduit_connect_realm,
            network::conduit_disconnect_realm,
            network::conduit_get_network_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
