import { describe, expect, it } from "vitest";
import { decodeModel, encodeModel, exportFileName, formatTokens, groupByDate, totalUsage } from "./format";

describe("groupByDate", () => {
  const now = new Date(2026, 10, 8, 15, 0, 0); // 8 nov 2026, 15:00
  const at = (y: number, m: number, d: number, h = 12) => ({ updatedAt: new Date(y, m, d, h).toISOString() });

  it("agrupa en Hoy, Ayer, 7 días, 30 días y Anteriores, sin grupos vacíos", () => {
    const items = [at(2026, 10, 8, 9), at(2026, 10, 7), at(2026, 10, 3), at(2026, 9, 20), at(2025, 0, 1)];
    const groups = groupByDate(items, now);
    expect(groups.map((g) => g.label)).toEqual(["Hoy", "Ayer", "Últimos 7 días", "Últimos 30 días", "Anteriores"]);
    expect(groups.every((g) => g.items.length === 1)).toBe(true);
  });

  it("mantiene el orden dentro de cada grupo y omite los vacíos", () => {
    const a = at(2026, 10, 8, 14);
    const b = at(2026, 10, 8, 8);
    const groups = groupByDate([a, b], now);
    expect(groups).toHaveLength(1);
    expect(groups[0].items).toEqual([a, b]);
  });

  it("devuelve una lista vacía si no hay elementos", () => {
    expect(groupByDate([], now)).toEqual([]);
  });
});

describe("formatTokens", () => {
  it("abrevia miles y millones con coma decimal", () => {
    expect(formatTokens(0)).toBe("0");
    expect(formatTokens(999)).toBe("999");
    expect(formatTokens(1234)).toBe("1,2k");
    expect(formatTokens(56_000)).toBe("56k");
    expect(formatTokens(2_500_000)).toBe("2,5M");
  });
});

describe("totalUsage", () => {
  it("suma tokens ignorando los valores nulos", () => {
    expect(
      totalUsage([
        { inputTokens: 10, outputTokens: 5 },
        { inputTokens: null, outputTokens: null },
        { inputTokens: 3, outputTokens: 7 },
      ]),
    ).toEqual({ input: 13, output: 12 });
  });
});

describe("encodeModel / decodeModel", () => {
  it("ida y vuelta, incluso con '::' o '/' en el nombre del modelo", () => {
    for (const model of ["abliterated-model", "openai/gpt-x", "a::b"]) {
      expect(decodeModel(encodeModel("compatible", model))).toEqual({ provider: "compatible", model });
    }
  });

  it("rechaza valores mal formados", () => {
    expect(decodeModel("")).toBeNull();
    expect(decodeModel("::modelo")).toBeNull();
    expect(decodeModel("anthropic::")).toBeNull();
    expect(decodeModel("sin-separador")).toBeNull();
  });
});

describe("exportFileName", () => {
  it("quita acentos y caracteres no válidos en Windows", () => {
    expect(exportFileName("Explicación: ¿qué es <Rust>?")).toBe("Explicacion-¿que-es-Rust.md");
    expect(exportFileName("a/b\\c|d")).toBe("abcd.md");
  });

  it("usa un nombre por defecto si queda vacío", () => {
    expect(exportFileName("???")).toBe("conversacion.md");
  });
});
