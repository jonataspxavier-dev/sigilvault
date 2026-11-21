// Ícones originais do SigilVault em SVG: traço único de 1,7 px, cantos arredondados, desenhados
// para serem reconhecíveis a 16 px. Nada de bibliotecas de ícones prontas.
// Original SigilVault SVG icons: single 1.7 px stroke, round joins, drawn to stay recognizable
// at 16 px. No ready-made icon libraries.

const SVG_NS = "http://www.w3.org/2000/svg";

type Shape = ["path", string] | ["circle", number, number, number] | ["rect", number, number, number, number, number];

const ICONS = {
  // Lupa com cabo de ferro / Magnifier with iron grip
  search: [["circle", 10, 10, 6], ["path", "M7.2 7.8a3.4 3.4 0 0 1 3.4-1.9"], ["path", "M14.4 14.4l1.6 1.6"], ["path", "M15.6 17.8l2.2-2.2 4 4-2.2 2.2z"]],
  // Estrela de oito pontas (novo) / Eight-point star (new)
  plus: [["path", "M12 3l1.5 6.5L20 8l-4.6 4 4.6 4-6.5-1.5L12 21l-1.5-6.5L4 16l4.6-4L4 8l6.5 1.5z"]],
  // Pena (gerar) / Quill (generate)
  quill: [["path", "M20.5 3.5C13 4 8 8.5 6.6 15l-1.1 4.5L9 17c6-1.5 10.5-6 11.5-13.5z"], ["path", "M17 7 6 18"], ["path", "M3 21.5h6"]],
  // Selo de cera com runa e fitas (salvar) / Wax seal with rune and ribbons (save)
  seal: [["circle", 12, 9.5, 6.5], ["circle", 12, 9.5, 3.6], ["path", "M12 7v5.5M10 9l2-2 2 2"], ["path", "M8.4 14.8 7 21.5l2.6-1.3L11 22M15.6 14.8 17 21.5l-2.6-1.3L13 22"]],
  // Pergaminho duplo com dobra (copiar) / Double parchment with fold (copy)
  copy: [["path", "M8.5 3h7.5l3 3v11H8.5z"], ["path", "M16 3v3h3"], ["path", "M5 7v13.5h10.5"], ["path", "M11 9.5h5M11 12.5h5M11 15h3"]],
  // Olho ornamentado / Ornate eye
  eye: [["path", "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z"], ["circle", 12, 12, 3], ["path", "M12 2.5v1.5M6.5 4l.8 1.3M17.5 4l-.8 1.3"]],
  eyeOff: [["path", "M2 12s3.6-7 10-7c2 0 3.7.7 5.1 1.6M22 12s-3.6 7-10 7c-2 0-3.7-.7-5.1-1.6"], ["path", "M9.9 14.1a3 3 0 0 0 4.2-4.2"], ["path", "M3 21 21 3"]],
  // Pena de escrever (editar) / Writing quill (edit)
  edit: [["path", "M4 20h4L19 9l-4-4L4 16v4z"], ["path", "M13.5 6.5l4 4"], ["path", "M6.5 17.5l2-2"]],
  trash: [["path", "M4 7h16M9 7V4.5h6V7M6 7l1 13h10l1-13M10 11v6M14 11v6"]],
  // Cadeado de base pontuda (bloquear) / Pointed-base padlock (lock)
  lock: [["path", "M5 11h14v6.2L12 22l-7-4.8z"], ["path", "M8 11V7.6a4 4 0 0 1 8 0V11"], ["circle", 12, 14.6, 1.4], ["path", "M12 16v2.6"], ["path", "M5 14h1.4M17.6 14H19"]],
  // Chave ornamentada (login) / Ornate key (login)
  key: [["path", "M9.6 9.6 20.5 20.5"], ["path", "M15.4 15.4l-2.3 2.3M18 18l-2.3 2.3"], ["circle", 7, 7, 3.8], ["circle", 7, 7, 1.3], ["path", "M7 1.6v1.6M1.6 7h1.6M3.2 3.2l1.1 1.1"]],
  // Pergaminho enrolado (nota segura) / Rolled scroll (secure note)
  scroll: [["path", "M7 7.5v9h10v-9"], ["path", "M5 4.5h14a1.5 1.5 0 0 1 0 3H5a1.5 1.5 0 0 1 0-3z"], ["path", "M5 16.5h14a1.5 1.5 0 0 1 0 3H5a1.5 1.5 0 0 1 0-3z"], ["path", "M9.5 10.5h5M9.5 13.2h3.2"]],
  // Usuário: cabeça e ombros / User: head and shoulders
  user: [["circle", 12, 8, 3.6], ["path", "M4.5 20c.8-3.8 3.9-6 7.5-6s6.7 2.2 7.5 6"]],
  // Escudo com cruz (programa) / Shield with cross (program)
  shield: [["path", "M4.5 3.5h15V11c0 5-3.4 8.6-7.5 10.3C7.9 19.6 4.5 16 4.5 11z"], ["path", "M12 6.5v11.5M7.5 10.5h9"]],
  // Rosa rúnica (configurações) / Runic compass (settings)
  gear: [["circle", 12, 12, 7], ["path", "M12 8.4l1.7 3.6L12 15.6 10.3 12z"], ["path", "M12 1.5v3M12 19.5v3M1.5 12h3M19.5 12h3"], ["path", "M6.7 6.7l1.4 1.4M15.9 15.9l1.4 1.4M6.7 17.3l1.4-1.4M15.9 8.1l1.4-1.4"]],
  // Gema lapidada (saúde) / Cut gem (health)
  gem: [["path", "M7 4h10l4 5-9 12L3 9z"], ["path", "M3 9h18M9 4l3 5 3-5M12 9v12"]],
  refresh: [["path", "M20 11a8 8 0 1 0-2.3 5.7"], ["path", "M20 4v7h-7"]],
  close: [["path", "M6 6l12 12M18 6 6 18"]],
  check: [["path", "M5 12.5l4.5 4.5L19 7.5"]],
  back: [["path", "M15 5l-7 7 7 7"]],
  warning: [["path", "M12 3 2 20h20z"], ["path", "M12 10v4.5M12 17.2v.3"]],
  // Baú com seta (exportar e importar) / Chest with arrow (export and import)
  chest: [["path", "M4 11h16v8.5A1.5 1.5 0 0 1 18.5 21h-13A1.5 1.5 0 0 1 4 19.5z"], ["path", "M4 11V9a4 4 0 0 1 4-4h1M20 11V9a4 4 0 0 0-4-4h-1"], ["path", "M10.5 14.5h3v3h-3z"], ["path", "M12 1.5v6M9.5 5 12 7.5 14.5 5"]],
  print: [["path", "M7 9V3h10v6"], ["rect", 4, 9, 16, 8, 1.5], ["path", "M7 14h10v7H7z"]],
} satisfies Record<string, Shape[]>;

export type IconName = keyof typeof ICONS;

/** Cria um ícone. A cor vem do texto (currentColor). / Creates an icon. Color comes from text (currentColor). */
export function icon(name: IconName, size = 18): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.7");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("icon");
  for (const shape of ICONS[name] as Shape[]) {
    const el = document.createElementNS(SVG_NS, shape[0]);
    if (shape[0] === "path") {
      el.setAttribute("d", shape[1]);
    } else if (shape[0] === "circle") {
      el.setAttribute("cx", String(shape[1]));
      el.setAttribute("cy", String(shape[2]));
      el.setAttribute("r", String(shape[3]));
    } else {
      el.setAttribute("x", String(shape[1]));
      el.setAttribute("y", String(shape[2]));
      el.setAttribute("width", String(shape[3]));
      el.setAttribute("height", String(shape[4]));
      el.setAttribute("rx", String(shape[5]));
    }
    svg.appendChild(el);
  }
  return svg;
}
