import { describe, expect, it } from "vitest";
import { expectedKeyPrefix, misplacedKeyHint } from "./api";

const ABLITERATION = { name: "Abliteration AI", baseUrl: "https://api.abliteration.ai/v1" };
const OLLAMA = { name: "Ollama", baseUrl: "http://localhost:11434/v1" };

describe("expectedKeyPrefix", () => {
  it("conoce los prefijos de cada servicio", () => {
    expect(expectedKeyPrefix("anthropic", null)?.prefix).toBe("sk-ant-");
    expect(expectedKeyPrefix("openai", null)?.prefix).toBe("sk-");
    expect(expectedKeyPrefix("compatible", ABLITERATION)?.prefix).toBe("ak_");
  });

  it("no inventa un prefijo para servicios desconocidos o sin configurar", () => {
    expect(expectedKeyPrefix("compatible", OLLAMA)).toBeNull();
    expect(expectedKeyPrefix("compatible", null)).toBeNull();
    expect(expectedKeyPrefix("compatible", { name: "x", baseUrl: "no es una url" })).toBeNull();
  });
});

describe("misplacedKeyHint", () => {
  it("detecta una key de Abliteration pegada en la fila de Anthropic u OpenAI", () => {
    expect(misplacedKeyHint("anthropic", "ak_123")).toContain("Compatible con OpenAI");
    expect(misplacedKeyHint("openai", "  ak_123  ")).toContain("Compatible con OpenAI");
  });

  it("detecta una key de Anthropic fuera de su fila", () => {
    expect(misplacedKeyHint("compatible", "sk-ant-api03-x")).toContain("Anthropic");
  });

  it("no avisa si la key está en su sitio o no se reconoce", () => {
    expect(misplacedKeyHint("compatible", "ak_123")).toBeNull();
    expect(misplacedKeyHint("anthropic", "sk-ant-123")).toBeNull();
    expect(misplacedKeyHint("openai", "sk-proj-123")).toBeNull();
  });
});
