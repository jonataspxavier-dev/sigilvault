// Ponto de entrada da interface: lê o estado do app e escolhe a tela.
// UI entry point: reads app state and picks the screen.

import "./styles.css";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";
import { setLang } from "./i18n";
import { session } from "./nav";
import { errorMessage } from "./ui";
import { showCreate, showRecover, showUnlock } from "./views/auth";
import { showPendingPairing, watchPairing } from "./pairing";
import { showVault } from "./views/vault";

async function boot(): Promise<void> {
  const state = await api.appState();
  session.settings = state.settings;
  session.remainingCodes = state.remainingCodes;
  setLang(state.settings.language);
  if (state.unlocked) {
    showVault();
    // A janela pode ter sido aberta justamente para mostrar um código. / The window may have opened just to show a code.
    void showPendingPairing();
  }
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

watchPairing();

// O cofre trancou ou abriu por outro caminho: redesenha a partir do estado real, para nunca
// mostrar "trancado" com o cofre aberto (nem o contrário). Ao abrir, só se a tela de desbloqueio
// estiver à mostra: assim nunca apaga a tela dos códigos de recuperação logo após criar o cofre.
// The vault locked or unlocked another way: redraws from the real state, so it never shows
// "locked" with the vault open (nor the reverse). On unlock, only if the unlock screen is showing:
// so it never wipes the recovery codes screen right after creating the vault.
void listen<boolean>("lock-state", (e) => {
  if (e.payload || document.querySelector('[data-screen="unlock"]')) void boot();
});

boot().catch((e) => {
  document.getElementById("app")!.textContent = errorMessage(e);
});
