// Navegação entre telas e estado compartilhado da interface (preferências).
// Screen navigation and shared UI state (preferences).

import type { AppState, Settings } from "./api";

export interface Nav {
  /** Relê o estado do app e decide a tela. / Re-reads app state and picks the screen. */
  boot(): Promise<void>;
  create(): void;
  unlock(state: AppState): void;
  recover(): void;
  vault(opts?: { rollbackWarning?: boolean }): void;
}

export const session: { settings: Settings | null; remainingCodes: number | null; nav: Nav | null } = {
  settings: null,
  remainingCodes: null,
  nav: null,
};

export function nav(): Nav {
  if (!session.nav) throw new Error("nav não iniciada / nav not initialized");
  return session.nav;
}
