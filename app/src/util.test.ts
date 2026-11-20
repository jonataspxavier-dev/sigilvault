import { describe, expect, it } from "vitest";
import { charClass, entropyLabel, formatCodeInput, parseTags, strengthLabel } from "./util";

describe("util", () => {
  it("classifica caracteres / classifies characters", () => {
    expect([..."aZ7#"].map(charClass)).toEqual(["lower", "upper", "digit", "symbol"]);
  });

  it("rótulos de entropia / entropy labels", () => {
    expect(entropyLabel(40)).toBe("entropy_weak");
    expect(entropyLabel(70)).toBe("entropy_fair");
    expect(entropyLabel(100)).toBe("entropy_strong");
    expect(entropyLabel(131)).toBe("entropy_excellent");
  });

  it("rótulos de força limitados a 0–4 / strength labels clamped to 0–4", () => {
    expect(strengthLabel(-3)).toBe("strength_0");
    expect(strengthLabel(3)).toBe("strength_3");
    expect(strengthLabel(9)).toBe("strength_4");
  });

  it("etiquetas sem vazios nem repetidas / tags without blanks or duplicates", () => {
    expect(parseTags(" banco, Pessoal,, banco ,PESSOAL , trabalho ")).toEqual(["banco", "Pessoal", "trabalho"]);
    expect(parseTags("")).toEqual([]);
  });

  it("formata o código de recuperação / formats the recovery code", () => {
    expect(formatCodeInput("k7qm2xpa9")).toBe("K7QM-2XPA-9");
    expect(formatCodeInput("K7QM-2XPA-9RTD-H4WN-6CZE-BJ3T-QAB-EXTRA")).toBe("K7QM-2XPA-9RTD-H4WN-6CZE-BJ3T-QAB");
  });
});
