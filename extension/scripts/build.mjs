// Monta a extensão para os navegadores suportados:
//   dist/chromium → Chrome, Brave e Opera
//   dist/firefox  → Firefox
// Passos: compila o WebAssembly do pareamento (Rust), o segundo plano e o popup (Vite), e grava
// um manifesto para cada família de navegador.
//
// Builds the extension for the supported browsers:
//   dist/chromium → Chrome, Brave and Opera
//   dist/firefox  → Firefox
// Steps: compiles the pairing WebAssembly (Rust), the background and the popup (Vite), and writes
// one manifest per browser family.

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "vite";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repo = resolve(root, "..");
const dist = join(root, "dist");
const common = join(dist, "common");
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));

// Chave pública do Chrome/Brave/Opera: fixa o ID `jclandjcanimgllfngognekhhhnomimf`, o mesmo de
// `sigilvault_ipc::CHROMIUM_EXTENSION_ID`. Só a chave pública: a privada não é guardada.
// Chrome/Brave/Opera public key: pins the ID `jclandjcanimgllfngognekhhhnomimf`, the same as
// `sigilvault_ipc::CHROMIUM_EXTENSION_ID`. Public key only: the private key is not kept.
const CHROMIUM_KEY =
  "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA4+y0etHhdtAIdamBfupBWvmiSHagxUj6mvMMKI5G1THA0+ShsWhPvw+jGVbDiZ8gLq9DZCB8Qia6YbxUpwR7CU7pHTSqD3ujvFMtURmMtxe9HXCYbeXtbmUD2gCEEaLfpnt+Gqdg+X51Y9VqkiDB0nXzLlOzbXuydqHZ9jl0IaG0djIWZaFYKLl6WbWmb4u9Y3DEVTKUqcY6DG9H1ayOR66yfYMvLp3CGppGJx2Sq+7kkzEAmSBDykUT9xF8PWMr4LjGA2XNjvKoTqTdzchrm6goG+9jhewI9vijlwgojphBiVH7mDQ10qV1GWlAXrcSed6CfRIssWuiScQxtK0gVwIDAQAB";
// Igual a `sigilvault_ipc::FIREFOX_EXTENSION_ID`. / Same as `sigilvault_ipc::FIREFOX_EXTENSION_ID`.
const FIREFOX_ID = "sigilvault@sigilvault.app";

const icons = (state) => Object.fromEntries(["16", "32", "48", "128"].map((s) => [s, `icons/${state}/${s}x${s}.png`]));

function manifest(target) {
  const base = {
    manifest_version: 3,
    name: "__MSG_ext_name__",
    description: "__MSG_ext_description__",
    default_locale: "en",
    version: pkg.version,
    // Só o Native Messaging: nenhum acesso a páginas nesta fase.
    // Native Messaging only: no access to web pages in this phase.
    permissions: ["nativeMessaging"],
    icons: icons("logo"),
    action: { default_popup: "popup/popup.html", default_title: "SigilVault", default_icon: icons("off") },
    // 'wasm-unsafe-eval' permite só WebAssembly (o pareamento), não eval de JavaScript.
    // 'wasm-unsafe-eval' allows only WebAssembly (pairing), not JavaScript eval.
    content_security_policy: { extension_pages: "script-src 'self' 'wasm-unsafe-eval'; object-src 'none'" },
  };
  if (target === "chromium") {
    return { ...base, key: CHROMIUM_KEY, minimum_chrome_version: "120", background: { service_worker: "background.js" } };
  }
  return {
    ...base,
    background: { scripts: ["background.js"] },
    browser_specific_settings: { gecko: { id: FIREFOX_ID, strict_min_version: "128.0", data_collection_permissions: { required: ["none"] } } },
  };
}

function cargo() {
  const local = join(homedir(), ".cargo", "bin", process.platform === "win32" ? "cargo.exe" : "cargo");
  return existsSync(local) ? local : "cargo";
}

rmSync(dist, { recursive: true, force: true });
mkdirSync(common, { recursive: true });

// 1. WebAssembly do pareamento. / Pairing WebAssembly.
const rust = spawnSync(cargo(), ["build", "-p", "sigilvault-pairing", "--target", "wasm32-unknown-unknown", "--release"], { cwd: repo, stdio: "inherit" });
if (rust.status !== 0) process.exit(rust.status ?? 1);
cpSync(join(repo, "target", "wasm32-unknown-unknown", "release", "sigilvault_pairing.wasm"), join(common, "pairing.wasm"));

// 2. Segundo plano: um arquivo só, sem módulos (funciona como service worker e no Firefox).
//    Background: a single file, no modules (works as a service worker and in Firefox).
await build({
  configFile: false,
  logLevel: "warn",
  publicDir: false,
  build: {
    outDir: common,
    emptyOutDir: false,
    target: "es2022",
    minify: false,
    lib: { entry: join(root, "src", "background.ts"), formats: ["iife"], name: "SigilVaultBackground", fileName: () => "background.js" },
  },
});

// 3. Popup + arquivos estáticos (ícones, textos do manifesto). / Popup + static files (icons, manifest strings).
await build({
  configFile: false,
  logLevel: "warn",
  root: join(root, "src"),
  base: "./",
  publicDir: join(root, "public"),
  build: { outDir: common, emptyOutDir: false, target: "es2022", minify: false, rollupOptions: { input: join(root, "src", "popup", "popup.html") } },
});

// 4. Uma pasta por família de navegador. / One folder per browser family.
for (const target of ["chromium", "firefox"]) {
  const out = join(dist, target);
  cpSync(common, out, { recursive: true });
  writeFileSync(join(out, "manifest.json"), JSON.stringify(manifest(target), null, 2) + "\n");
}
rmSync(common, { recursive: true, force: true });
console.log("Extensão pronta / Extension ready: dist/chromium, dist/firefox");
