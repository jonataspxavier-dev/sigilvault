// Roda o Vitest a partir do caminho "verdadeiro" da pasta. No Windows, abrir o terminal em "c:\"
// (minúsculo) faz o Vitest carregar o mesmo módulo duas vezes e falhar ("failed to find the
// current suite"). realpathSync.native devolve "C:\"; iniciamos o Vitest num processo novo a
// partir dele.
// Runs Vitest from the folder's "true" path. On Windows, a terminal opened at "c:\" (lowercase)
// makes Vitest load the same module twice and fail ("failed to find the current suite").
// realpathSync.native returns "C:\"; we start Vitest in a fresh process from there.
import { spawnSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { join } from "node:path";

const cwd = realpathSync.native(process.cwd());
const bin = join(cwd, "node_modules", "vitest", "vitest.mjs");
const result = spawnSync(process.execPath, [bin, "run", ...process.argv.slice(2)], { cwd, stdio: "inherit" });
process.exit(result.status ?? 1);
