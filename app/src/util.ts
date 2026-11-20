// Funções puras (sem tela), fáceis de testar.
// Pure functions (no UI), easy to test.

import type { Key } from "./i18n";

/** Classe visual de cada caractere da senha, para colorir números e símbolos.
 *  Visual class of each password character, to color digits and symbols. */
export type CharClass = "lower" | "upper" | "digit" | "symbol";

export function charClass(c: string): CharClass {
  if (/[a-z]/.test(c)) return "lower";
  if (/[A-Z]/.test(c)) return "upper";
  if (/[0-9]/.test(c)) return "digit";
  return "symbol";
}

/** Rótulo da entropia em bits. / Entropy label in bits. */
export function entropyLabel(bits: number): Key {
  if (bits < 60) return "entropy_weak";
  if (bits < 80) return "entropy_fair";
  if (bits < 110) return "entropy_strong";
  return "entropy_excellent";
}

/** Rótulo da nota do zxcvbn (0–4). / zxcvbn score label (0–4). */
export function strengthLabel(score: number): Key {
  const s = Math.max(0, Math.min(4, Math.round(score)));
  return `strength_${s}` as Key;
}

/** "a, b ,, c" → ["a", "b", "c"], sem repetidos. / without duplicates. */
export function parseTags(text: string): string[] {
  const out: string[] = [];
  for (const raw of text.split(",")) {
    const tag = raw.trim();
    if (tag && !out.some((t) => t.toLowerCase() === tag.toLowerCase())) out.push(tag);
  }
  return out;
}

/** Deixa o código no formato XXXX-XXXX-… enquanto o usuário digita.
 *  Formats the code as XXXX-XXXX-… while the user types. */
export function formatCodeInput(text: string): string {
  const clean = text.toUpperCase().replace(/[^0-9A-Z]/g, "").slice(0, 27);
  return clean.replace(/(.{4})(?=.)/g, "$1-");
}

/** Data local a partir de segundos Unix. / Local date from Unix seconds. */
export function formatDate(unixSecs: number, lang: "pt" | "en"): string {
  return new Date(unixSecs * 1000).toLocaleString(lang === "pt" ? "pt-BR" : "en-US", {
    dateStyle: "medium",
    timeStyle: "short",
  });
}
