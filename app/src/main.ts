// Ponto de entrada da interface: lê o estado do app e escolhe a tela.
// UI entry point: reads app state and picks the screen.

import "./styles.css";
import { api } from "./api";
import { setLang } from "./i18n";
import { session } from "./nav";
import { errorMessage } from "./ui";
import { showCreate, showRecover, showUnlock } from "./views/auth";
import { showVault } from "./views/vault";

async function boot(): Promise<void> {
  const state = await api.appState();
  session.settings = state.settings;
  session.remainingCodes = state.remainingCodes;
  setLang(state.settings.language);
  if (state.unlocked) showVault();
  else if (state.hasVault) showUnlock(state);
  else showCreate();
}

session.nav = {
  boot,
  create: showCreate,
  unlock: showUnlock,
  recover: showRecover,
  vault: showVault,
};

// Bloqueia o menu de contexto padrão (evita "Inspecionar" e "Salvar como").
// Blocks the default context menu (avoids "Inspect" and "Save as").
document.addEventListener("contextmenu", (e) => {
  const target = e.target as HTMLElement;
  if (!target.closest("input, textarea")) e.preventDefault();
});

boot().catch((e) => {
  document.getElementById("app")!.textContent = errorMessage(e);
});
