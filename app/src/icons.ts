// Ícones originais do SigilVault em SVG: traço único de 1,8 px, cantos arredondados, desenhados
// para serem reconhecíveis a 16 px. Nada de bibliotecas de ícones prontas.
// Original SigilVault SVG icons: single 1.8 px stroke, round joins, drawn to stay recognizable
// at 16 px. No ready-made icon libraries.

const SVG_NS = "http://www.w3.org/2000/svg";

type Shape = ["path", string] | ["circle", number, number, number] | ["rect", number, number, number, number, number];

const ICONS = {
  // Lupa de latão / Brass magnifier
  search: [["circle", 10.5, 10.5, 6], ["path", "M15 15l5.5 5.5"], ["path", "M8 8.6a3 3 0 0 1 2.6-1.6"]],
  plus: [["path", "M12 5v14M5 12h14"]],
  // Pena (gerar) / Quill (generate)
  quill: [["path", "M20 3c-7 1-12 6-13.5 12.5L5 21l3.6-3.4C15 16 19 11 20 3z"], ["path", "M9.5 14.5l5-5"]],
  // Selo de cera (salvar) / Wax seal (save)
  seal: [["circle", 12, 10, 6], ["path", "M9.3 15.4 8 21.5l2.6-1.4 1.4 1.9 1.1-5.4"], ["path", "M10 8.2c.6-.9 2.8-1 3.4.2.6 1.4-3.4 1.6-2.8 3.2.5 1.2 2.6 1 3.2.2"]],
  // Pergaminho duplo (copiar) / Double parchment (copy)
  copy: [["rect", 8, 3, 11, 14, 1.5], ["path", "M5 7v12.5A1.5 1.5 0 0 0 6.5 21H15"], ["path", "M11 7.5h5M11 10.5h5M11 13.5h3"]],
  // Olho ornamentado / Ornate eye
  eye: [["path", "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z"], ["circle", 12, 12, 3], ["path", "M12 2.5v1.5M6.5 4l.8 1.3M17.5 4l-.8 1.3"]],
  eyeOff: [["path", "M2 12s3.6-7 10-7c2 0 3.7.7 5.1 1.6M22 12s-3.6 7-10 7c-2 0-3.7-.7-5.1-1.6"], ["path", "M9.9 14.1a3 3 0 0 0 4.2-4.2"], ["path", "M3 21 21 3"]],
  edit: [["path", "M4 20h4L19 9l-4-4L4 16v4z"], ["path", "M13.5 6.5l4 4"]],
  trash: [["path", "M4 7h16M9 7V4.5h6V7M6 7l1 13h10l1-13M10 11v6M14 11v6"]],
  // Cadeado de ferro / Iron padlock
  lock: [["rect", 5, 10, 14, 11, 2], ["path", "M8 10V7a4 4 0 0 1 8 0v3"], ["circle", 12, 15.5, 1.4]],
  // Chave antiga (login) / Old key (login)
  key: [["circle", 7.5, 15.5, 3.8], ["path", "M10.3 12.8 20 3M16.5 6.5l2.5 2.5M14 9l2 2"]],
  // Pergaminho (nota segura) / Scroll (secure note)
  scroll: [["path", "M7 4h11a2 2 0 0 1 2 2v1h-3"], ["path", "M17 6v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-1h11"], ["path", "M7 4a2 2 0 0 0-2 2v11M9 9h5M9 12h5"]],
  // Escudo (app desktop) / Shield (desktop app)
  shield: [["path", "M12 3l7 3v6c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6z"]],
  // Rosa dos ventos rúnica (configurações) / Runic compass (settings)
  gear: [["circle", 12, 12, 3.2], ["path", "M12 2.5v4M12 17.5v4M2.5 12h4M17.5 12h4M5.3 5.3l2.8 2.8M15.9 15.9l2.8 2.8M5.3 18.7l2.8-2.8M15.9 8.1l2.8-2.8"]],
  refresh: [["path", "M20 11a8 8 0 1 0-2.3 5.7"], ["path", "M20 4v7h-7"]],
  close: [["path", "M6 6l12 12M18 6 6 18"]],
  check: [["path", "M5 12.5l4.5 4.5L19 7.5"]],
  back: [["path", "M15 5l-7 7 7 7"]],
  warning: [["path", "M12 3 2 20h20z"], ["path", "M12 10v4.5M12 17.2v.3"]],
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
  svg.setAttribute("stroke-width", "1.8");
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
