// Chamadas tipadas aos comandos Rust do app. Nenhuma criptografia acontece aqui.
// Typed calls to the app's Rust commands. No cryptography happens here.

import { invoke } from "@tauri-apps/api/core";

export type Lang = "pt" | "en";
export type ItemKind = "login" | "secure_note";

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
}

/** Erro vindo do Rust: só um código, nunca segredos. / Error from Rust: just a code, never secrets. */
export interface CmdError {
  code: string;
  retryAfterSecs?: number;
}

export function isCmdError(e: unknown): e is CmdError {
  return typeof e === "object" && e !== null && typeof (e as { code?: unknown }).code === "string";
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
};
