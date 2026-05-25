#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod access_commands;
mod node_state;
mod relay_state;

use node_state::NodeState;
use relay_state::RelayState;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(NodeState::new())
        .manage(RelayState::new())
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
            commands::get_invite_key,
            commands::create_join_request,
            commands::accept_join_request,
            commands::apply_join_response,
            commands::share_file,
            commands::get_share_info,
            commands::revoke_share,
            commands::set_share_public,
            commands::pull_shared_file,
            commands::list_received_files,
            commands::decrypt_received,
            commands::remove_received,
            commands::push_to_peer,
            commands::start_node,
            commands::stop_node,
            commands::get_node_info,
            commands::get_config,
            commands::save_config,
            commands::delete_file,
            commands::rename_file,
            commands::get_connectivity_stats,
            commands::export_file_bundle,
            commands::import_file_bundle,
            commands::list_shards,
            commands::verify_store,
            commands::start_relay,
            commands::stop_relay,
            commands::get_relay_info,
            // Access control commands
            access_commands::acl_contact_add,
            access_commands::acl_contact_remove,
            access_commands::acl_contact_list,
            access_commands::acl_contact_get,
            access_commands::acl_contact_set_access,
            access_commands::acl_group_create,
            access_commands::acl_group_delete,
            access_commands::acl_group_list,
            access_commands::acl_group_get,
            access_commands::acl_group_add_member,
            access_commands::acl_group_remove_member,
            access_commands::acl_folder_create,
            access_commands::acl_folder_remove,
            access_commands::acl_folder_list,
            access_commands::acl_folder_get,
            access_commands::acl_folder_grant,
            access_commands::acl_folder_revoke,
            access_commands::acl_check_permission,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NEXUS");
}
