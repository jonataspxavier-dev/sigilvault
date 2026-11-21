// Prepara o que o instalador leva além do app: o conector da extensão (sigilvault-host.exe, versão
// final) e a extensão compilada. Rodado pelo `tauri build` (beforeBuildCommand).
// Prepares what the installer ships besides the app: the extension connector (sigilvault-host.exe,
// release) and the built extension. Run by `tauri build` (beforeBuildCommand).

import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const app = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repo = resolve(app, "..");
const cargoLocal = join(homedir(), ".cargo", "bin", "cargo.exe");
const cargo = existsSync(cargoLocal) ? cargoLocal : "cargo";

function run(cmd, args, cwd) {
  const r = spawnSync(cmd, args, { cwd, stdio: "inherit", shell: cmd === "npm" });
  if (r.status !== 0) process.exit(r.status ?? 1);
}

// 1. Conector, versão final. / Connector, release build.
run(cargo, ["build", "-p", "sigilvault-host", "--release"], repo);
const binaries = join(app, "src-tauri", "binaries");
mkdirSync(binaries, { recursive: true });
// O Tauri exige o sufixo da plataforma; no instalador vira "sigilvault-host.exe".
// Tauri requires the platform suffix; in the installer it becomes "sigilvault-host.exe".
copyFileSync(join(repo, "target", "release", "sigilvault-host.exe"), join(binaries, "sigilvault-host-x86_64-pc-windows-msvc.exe"));

// 2. Extensão. / Extension.
run("npm", ["run", "build"], join(repo, "extension"));
console.log("Pacote preparado / Bundle prepared");
