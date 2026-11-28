// Prepara o que o pacote leva além do app: o conector da extensão (sigilvault-host, versão final)
// e a extensão compilada. Rodado pelo `tauri build` (beforeBuildCommand). Funciona no Windows e no Linux.
// Prepares what the package ships besides the app: the extension connector (sigilvault-host,
// release) and the built extension. Run by `tauri build` (beforeBuildCommand). Works on Windows and Linux.

import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const app = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repo = resolve(app, "..");
const exe = process.platform === "win32" ? ".exe" : "";
const cargoLocal = join(homedir(), ".cargo", "bin", `cargo${exe}`);
const cargo = existsSync(cargoLocal) ? cargoLocal : "cargo";

function run(cmd, args, cwd) {
  const r = spawnSync(cmd, args, { cwd, stdio: "inherit", shell: cmd === "npm" });
  if (r.status !== 0) process.exit(r.status ?? 1);
}

// 1. Conector, versão final. / Connector, release build.
run(cargo, ["build", "-p", "sigilvault-host", "--release"], repo);
const binaries = join(app, "src-tauri", "binaries");
mkdirSync(binaries, { recursive: true });
// O Tauri exige o sufixo da plataforma (o "triplo" do compilador); no pacote ele some.
// Tauri requires the platform suffix (the compiler's "triple"); in the package it is dropped.
const rustc = existsSync(join(homedir(), ".cargo", "bin", `rustc${exe}`)) ? join(homedir(), ".cargo", "bin", `rustc${exe}`) : "rustc";
const info = spawnSync(rustc, ["-vV"], { encoding: "utf8" });
const triple = /^host: (.+)$/m.exec(info.stdout ?? "")?.[1]?.trim();
if (info.status !== 0 || !triple) {
  console.error("Não consegui descobrir o alvo do Rust / Could not find the Rust target");
  process.exit(1);
}
copyFileSync(join(repo, "target", "release", `sigilvault-host${exe}`), join(binaries, `sigilvault-host-${triple}${exe}`));

// 2. Extensão. / Extension.
run("npm", ["run", "build"], join(repo, "extension"));
console.log("Pacote preparado / Bundle prepared");
