//! Gera as permissões dos comandos do app: cada comando precisa ser liberado em `capabilities/`.
//! Generates app command permissions: each command must be allowed in `capabilities/`.

include!("src/command_list.rs");

/// O Tauri exige que o conector (`externalBin`) exista, mas ele só é montado por
/// `scripts/prepare-bundle.mjs`. Em desenvolvimento e nos testes, deixa um arquivo no lugar, para
/// `cargo check` e `cargo test` funcionarem num clone limpo: uma cópia do conector já compilado,
/// se houver (o `tauri dev` copia esse arquivo para junto do app, então ele não pode ficar vazio
/// quando o conector existe), ou um arquivo vazio. Na versão final não cria nada: o empacotamento
/// falha em vez de levar um conector vazio.
/// Tauri requires the connector (`externalBin`) to exist, but it is only assembled by
/// `scripts/prepare-bundle.mjs`. In development and tests, leaves a file in its place, so
/// `cargo check` and `cargo test` work on a clean clone: a copy of the already-built connector,
/// if any (`tauri dev` copies this file next to the app, so it must not stay empty when the
/// connector exists), or an empty file. In release it creates nothing: packaging fails instead of
/// shipping an empty connector.
fn placeholders() {
    if std::env::var("PROFILE").is_ok_and(|p| p == "release") {
        return;
    }
    let Ok(target) = std::env::var("TARGET") else { return };
    let ext = if target.contains("windows") { ".exe" } else { "" };
    let file = std::path::Path::new("binaries").join(format!("sigilvault-host-{target}{ext}"));
    // `<alvo>/<perfil>/build/<pacote>/<hash>/out` → a pasta do perfil, onde o cargo põe o conector.
    // `<target>/<profile>/build/<package>/<hash>/out` → the profile folder, where cargo puts the connector.
    let built = std::env::var_os("OUT_DIR")
        .and_then(|out| std::path::PathBuf::from(out).ancestors().nth(4).map(|dir| dir.join(format!("sigilvault-host{ext}"))))
        .and_then(|path| std::fs::read(path).ok())
        .filter(|bytes| !bytes.is_empty());
    let current_is_empty = std::fs::metadata(&file).map(|m| m.len() == 0).unwrap_or(true);
    if current_is_empty {
        let _ = std::fs::create_dir_all("binaries");
        let _ = std::fs::write(&file, built.unwrap_or_default());
    }
    // O `tauri dev` copia o arquivo com as mesmas permissões: precisa poder ser executado.
    // `tauri dev` copies the file with the same permissions: it must be executable.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755));
    }
    // O mesmo vale para a extensão compilada, que o instalador leva como recurso.
    // The same goes for the built extension, which the installer ships as a resource.
    for family in ["chromium", "firefox"] {
        let _ = std::fs::create_dir_all(std::path::Path::new("../../extension/dist").join(family));
    }
}

fn main() {
    placeholders();
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("falha no build do Tauri / Tauri build failed");
}
