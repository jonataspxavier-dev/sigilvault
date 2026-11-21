// Chamadas tipadas aos comandos Rust do app. Nenhuma criptografia acontece aqui.
// Typed calls to the app's Rust commands. No cryptography happens here.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Lang = "pt" | "en";
export type ItemKind = "login" | "secure_note" | "program";

export interface GeneratorPrefs {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  excludeAmbiguous: boolean;
}

export interface Settings {
  language: Lang;
  autoLockMinutes: number;
  clipboardClearSecs: number;
  generator: GeneratorPrefs;
  /** Atalhos globais ("" = desligado). / Global hotkeys ("" = off). */
  hotkeySearch: string;
  hotkeyGenerate: string;
  /** Abrir na bandeja ao entrar no Windows. / Open in the tray at Windows sign-in. */
  startWithWindows: boolean;
}

export interface AppState {
  hasVault: boolean;
  unlocked: boolean;
  username: string | null;
  remainingCodes: number | null;
  retryAfterSecs: number;
  settings: Settings;
}

export interface Item {
  id: string;
  kind: ItemKind;
  title: string;
  username: string;
  url: string;
  tags: string[];
  hasPassword: boolean;
  hasNotes: boolean;
  createdAt: number;
  updatedAt: number;
  matchSubdomains: boolean;
  historyLen: number;
  /** Programa: caminho do executável. / Program: executable path. */
  appPath: string;
  appSigned: boolean;
}

/** Em edição: `password`/`notes` ausentes = manter, "" = remover. / On edit: missing = keep, "" = remove. */
export interface ItemInput {
  kind: ItemKind;
  title: string;
  username: string;
  url: string;
  tags: string[];
  password?: string;
  notes?: string;
  matchSubdomains?: boolean;
  appPath?: string;
}

export interface ImportResult {
  format: string;
  added: number;
  duplicates: number;
  skipped: number;
  plaintext: boolean;
}

export interface BackupEntry {
  id: string;
  createdAt: number;
  revision: number;
  itemCount: number;
}

/** Erro vindo do Rust: só um código, nunca segredos. / Error from Rust: just a code, never secrets. */
export interface CmdError {
  code: string;
  retryAfterSecs?: number;
}

export function isCmdError(e: unknown): e is CmdError {
  return typeof e === "object" && e !== null && typeof (e as { code?: unknown }).code === "string";
}

/** Pareamento pedido por uma extensão, com o código para mostrar. / Pairing requested by an extension, with the code to show. */
export interface PairingPending {
  code: string;
  browser: string;
  expiresInSecs: number;
  attemptsLeft: number;
}

/** Navegador pareado. / Paired browser. */
export interface PairingEntry {
  id: string;
  browser: string;
  createdAt: number;
  lastSeen: number;
  connected: boolean;
  /** Sites liberados para preencher sem perguntar. / Sites allowed to fill without asking. */
  sites: string[];
}

/** Um preenchimento feito pela extensão. / A fill made by the extension. */
export interface FillEntry {
  at: number;
  browser: string;
  origin: string;
  title: string;
  username: string;
}

/** Aviso do app sobre o pareamento. / App notice about pairing. */
export interface PairingEvent {
  event: "pending" | "paired" | "cancelled";
  browser: string | null;
}

/** Pop-up rápido: para onde vai digitar. / Quick popup: where it will type. */
export interface QuickContext {
  mode: "search" | "generate";
  target: { exeName: string; exePath: string; title: string; browser: boolean; elevated: boolean } | null;
  unlocked: boolean;
  username: string | null;
  language: Lang;
  generator: GeneratorPrefs;
}

export interface QuickItem {
  id: string;
  title: string;
  username: string;
  kind: ItemKind;
  matched: boolean;
}

export const api = {
  appState: () => invoke<AppState>("app_state"),
  passwordStrength: (password: string, username: string) => invoke<number>("password_strength", { password, username }),
  createVault: (username: string, password: string) => invoke<string[]>("create_vault", { username, password }),
  unlock: (password: string) => invoke<{ rollbackWarning: boolean; remainingCodes: number }>("unlock", { password }),
  recover: (code: string, newPassword: string) =>
    invoke<{ username: string; remainingCodes: number }>("recover", { code, newPassword }),
  lock: () => invoke<void>("lock"),
  listItems: (query: string) => invoke<Item[]>("list_items", { query }),
  addItem: (item: ItemInput) => invoke<string>("add_item", { item }),
  updateItem: (id: string, item: ItemInput) => invoke<void>("update_item", { id, item }),
  deleteItem: (id: string) => invoke<void>("delete_item", { id }),
  revealPassword: (id: string) => invoke<string | null>("reveal_password", { id }),
  revealNotes: (id: string) => invoke<string | null>("reveal_notes", { id }),
  copyField: (id: string, field: "password" | "username") => invoke<number>("copy_field", { id, field }),
  copyText: (text: string) => invoke<number>("copy_text", { text }),
  generatePassword: (options: GeneratorPrefs) =>
    invoke<{ password: string; entropyBits: number }>("generate_password", { options }),
  changeMasterPassword: (current: string, next: string) => invoke<void>("change_master_password", { current, new: next }),
  regenerateCodes: (password: string) => invoke<string[]>("regenerate_codes", { password }),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  // Fase 3: os caminhos vêm de diálogos nativos abertos pelo Rust. / Phase 3: paths come from native dialogs opened by Rust.
  exportVault: (mode: "copy" | "password", password?: string) => invoke<boolean>("export_vault", { mode, password }),
  exportCsv: (masterPassword: string) => invoke<boolean>("export_csv", { masterPassword }),
  importFile: () => invoke<ImportResult | null>("import_file"),
  deleteImportedFile: () => invoke<boolean>("delete_imported_file"),
  mergeVault: (password: string) => invoke<ImportResult | null>("merge_vault", { password }),
  importVaultFile: () => invoke<boolean>("import_vault_file"),
  listBackups: () => invoke<BackupEntry[]>("list_backups"),
  restoreBackup: (id: string) => invoke<boolean>("restore_backup", { id }),
  // Fase 4: extensão do navegador. / Phase 4: browser extension.
  pairingPending: () => invoke<PairingPending | null>("pairing_pending"),
  cancelPairing: () => invoke<void>("cancel_pairing"),
  listPairings: () => invoke<PairingEntry[]>("list_pairings"),
  removePairing: (id: string) => invoke<void>("remove_pairing", { id }),
  // Fase 5. / Phase 5.
  revokeSite: (id: string, site: string) => invoke<void>("revoke_site", { id, site }),
  fillLog: () => invoke<FillEntry[]>("fill_log"),
  // Fase 6. / Phase 6.
  quickContext: () => invoke<QuickContext>("quick_context"),
  quickSearch: (query: string) => invoke<QuickItem[]>("quick_search", { query }),
  quickType: (id: string) => invoke<void>("quick_type", { id }),
  quickSaveProgram: (title: string, username: string, password: string, typeIt: boolean) =>
    invoke<string>("quick_save_program", { title, username, password, typeIt }),
  quickClose: () => invoke<void>("quick_close"),
  hotkeysInUse: () => invoke<("search" | "generate")[]>("hotkeys_in_use"),
  revealHistory: (id: string) => invoke<{ changedAt: number; password: string }[]>("reveal_history", { id }),
  onPairing: (cb: (e: PairingEvent) => void) => listen<PairingEvent>("pairing", (e) => cb(e.payload)),
};
