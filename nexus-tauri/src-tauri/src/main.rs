#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running NEXUS");
}
