// Painel do gerador de senhas. A senha é sorteada no Rust (gerador do sistema, sem viés).
// Password generator panel. The password is drawn in Rust (system RNG, unbiased).

import { api, type GeneratorPrefs } from "../api";
import { h } from "../dom";
import { t } from "../i18n";
import { session } from "../nav";
import { button, coloredSecret, errorBox, errorMessage, toast } from "../ui";
import { entropyLabel } from "../util";

/** `minimal`: sem "copiar" e "salvar como padrão" (o pop-up rápido não tem essas permissões).
 *  `minimal`: no "copy" and "save as default" (the quick popup lacks those permissions). */
export function generatorPanel(opts: { onUse?: (password: string) => void; compact?: boolean; minimal?: boolean; useLabel?: string } = {}): HTMLElement {
  const prefs: GeneratorPrefs = { ...session.settings!.generator };
  let current = "";

  const output = h("div", { class: "gen-output", "aria-live": "polite" });
  const entropy = h("div", { class: "gen-entropy muted" });
  const err = errorBox();

  const lengthLabel = h("span", { class: "field-label" });
  const slider = h("input", { type: "range", min: 4, max: 128, value: String(prefs.length), class: "slider" });
  const toggle = (key: Exclude<keyof GeneratorPrefs, "length">, label: string) => {
    const box = h("input", { type: "checkbox", checked: prefs[key] });
    box.addEventListener("change", () => {
      prefs[key] = box.checked;
      void generate();
    });
    return h("label", { class: "check" }, box, h("span", null, label));
  };

  async function generate() {
    lengthLabel.textContent = t("gen_length", { n: prefs.length });
    err.clear();
    try {
      const res = await api.generatePassword(prefs);
      current = res.password;
      output.replaceChildren(coloredSecret(current));
      entropy.textContent = t("gen_entropy", { bits: Math.round(res.entropyBits), label: t(entropyLabel(res.entropyBits)) });
    } catch (e) {
      current = "";
      output.replaceChildren();
      entropy.textContent = "";
      err.show(errorMessage(e));
    }
  }

  slider.addEventListener("input", () => {
    prefs.length = Number(slider.value);
    void generate();
  });

  const actions = h(
    "div",
    { class: "actions" },
    button(t("gen_again"), { icon: "refresh", onClick: () => void generate() }),
    opts.minimal ? null : button(t("copy"), {
      icon: "copy",
      onClick: async () => {
        if (!current) return;
        try {
          toast(t("copied", { secs: await api.copyText(current) }));
        } catch (e) {
          err.show(errorMessage(e));
        }
      },
    }),
    opts.onUse ? button(opts.useLabel ?? t("gen_use"), { icon: "check", variant: "primary", onClick: () => current && opts.onUse!(current) }) : null,
  );

  const saveDefault = h(
    "button",
    {
      type: "button",
      class: "link",
      onClick: async () => {
        try {
          session.settings = await api.saveSettings({ ...session.settings!, generator: { ...prefs } });
          toast(t("gen_saved_default"));
        } catch (e) {
          err.show(errorMessage(e));
        }
      },
    },
    t("gen_save_default"),
  );

  const panel = h(
    "div",
    { class: opts.compact ? "generator generator-compact" : "generator" },
    output,
    entropy,
    h("label", { class: "field" }, lengthLabel, slider, h("span", { class: "field-hint" }, t("gen_sites_hint"))),
    h(
      "div",
      { class: "gen-options" },
      toggle("lowercase", t("gen_lowercase")),
      toggle("uppercase", t("gen_uppercase")),
      toggle("digits", t("gen_digits")),
      toggle("symbols", t("gen_symbols")),
      toggle("excludeAmbiguous", t("gen_exclude")),
    ),
    err.el,
    actions,
    opts.minimal ? null : saveDefault,
  );
  void generate();
  return panel;
}
