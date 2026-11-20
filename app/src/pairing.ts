// Pareamento com a extensão: quando um navegador pede para parear, o app mostra o código que o
// usuário digita na extensão. Ver o código na tela É a confirmação do usuário.
// Pairing with the extension: when a browser asks to pair, the app shows the code the user types
// in the extension. Seeing the code on screen IS the user's confirmation.

import { api } from "./api";
import { h } from "./dom";
import { t } from "./i18n";
import { button, modal, toast } from "./ui";

let open: { close(): void } | null = null;
let timer: number | undefined;

function closeDialog(): void {
  window.clearInterval(timer);
  open?.close();
  open = null;
}

/** Mostra o código pendente, se houver (só funciona com o cofre aberto). / Shows the pending code, if any (only with the vault unlocked). */
export async function showPendingPairing(): Promise<void> {
  const pending = await api.pairingPending().catch(() => null);
  closeDialog();
  if (!pending) return;

  let left = pending.expiresInSecs;
  const countdown = h("p", { class: "muted" });
  const tick = () => {
    if (left <= 0) {
      closeDialog();
      void api.cancelPairing();
      return;
    }
    countdown.textContent = t("pair_expires", { secs: left });
    left -= 1;
  };
  tick();
  timer = window.setInterval(tick, 1000);

  open = modal(
    t("pair_title"),
    [
      h("p", null, t("pair_intro", { browser: pending.browser })),
      h("p", { class: "pair-code", "aria-label": t("pair_code_label") }, pending.code),
      countdown,
      h("p", { class: "muted" }, t("pair_warning")),
    ],
    [
      button(t("cancel"), {
        variant: "ghost",
        onClick: () => {
          closeDialog();
          void api.cancelPairing();
        },
      }),
    ],
  );
}

/** Ouve os avisos de pareamento do app. Chamar uma vez por janela. / Listens to the app's pairing notices. Call once per window. */
export function watchPairing(): void {
  void api.onPairing((e) => {
    if (e.event === "pending") void showPendingPairing();
    else {
      closeDialog();
      if (e.event === "paired") toast(t("pair_done", { browser: e.browser ?? "" }));
    }
  });
}
