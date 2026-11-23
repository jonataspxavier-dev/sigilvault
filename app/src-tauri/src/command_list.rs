// Lista única de comandos do app, incluída pelo `build.rs` (permissões) e por `commands.rs` (teste).
// Single list of app commands, included by `build.rs` (permissions) and `commands.rs` (test).
#[allow(dead_code)]
const COMMANDS: &[&str] = &[
    "app_state", "password_strength", "create_vault", "unlock", "recover", "lock", "list_items",
    "add_item", "update_item", "delete_item", "reveal_password", "reveal_notes", "copy_field",
    "copy_text", "generate_password", "change_master_password", "regenerate_codes", "save_settings",
    "export_vault", "export_csv", "import_file", "delete_imported_file", "merge_vault", "import_vault_file",
    "list_backups", "restore_backup", "pairing_pending", "cancel_pairing", "list_pairings", "remove_pairing",
    "revoke_site", "fill_log", "reveal_history", "health_report",
    "quick_context", "quick_search", "quick_type", "quick_save_program", "quick_close", "hotkeys_in_use", "quick_offer_accept", "quick_ignore_program",
];
