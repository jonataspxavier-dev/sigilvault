//! Gera as permissões dos comandos do app: cada comando precisa ser liberado em `capabilities/`.
//! Generates app command permissions: each command must be allowed in `capabilities/`.

include!("src/command_list.rs");

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("falha no build do Tauri / Tauri build failed");
}
