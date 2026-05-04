#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod node_state;

use node_state::NodeState;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(NodeState::new())
        .invoke_handler(tauri::generate_handler![
            commands::create_identity,
            commands::get_identity,
            commands::encrypt_file,
            commands::decrypt_file,
            commands::get_store_stats,
            commands::list_files,
            commands::add_contact,
            commands::list_contacts,
            commands::remove_contact,
            commands::update_contact,
            commands::share_file,
            commands::queue_send,
            commands::list_send_queue,
            commands::cancel_send,
            commands::retry_send,
            commands::list_received_files,
            commands::decrypt_received,
            commands::remove_received,
            commands::start_node,
            commands::stop_node,
            commands::get_node_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NEXUS");
}
