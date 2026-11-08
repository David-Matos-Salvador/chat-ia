/** Utilidades de presentación sin dependencias (fáciles de probar). */

export interface DatedItem {
  updatedAt: string;
}

export interface Group<T> {
  label: string;
  items: T[];
}

const DAY = 24 * 60 * 60 * 1000;

function startOfDay(d: Date): number {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/**
 * Agrupa por fecha de última actividad: Hoy, Ayer, Últimos 7 días, Últimos 30 días, Anteriores.
 * Mantiene el orden de entrada dentro de cada grupo y omite los grupos vacíos.
 */
export function groupByDate<T extends DatedItem>(items: T[], now: Date = new Date()): Group<T>[] {
  const today = startOfDay(now);
  const buckets: Group<T>[] = [
    { label: "Hoy", items: [] },
    { label: "Ayer", items: [] },
    { label: "Últimos 7 días", items: [] },
    { label: "Últimos 30 días", items: [] },
    { label: "Anteriores", items: [] },
  ];
  for (const item of items) {
    const t = new Date(item.updatedAt).getTime();
    const index =
      t >= today ? 0 : t >= today - DAY ? 1 : t >= today - 7 * DAY ? 2 : t >= today - 30 * DAY ? 3 : 4;
    buckets[index].items.push(item);
  }
  return buckets.filter((b) => b.items.length > 0);
}

/** 1234 → "1,2k"; 999 → "999". */
export function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0).replace(".", ",")}k`;
  return `${(n / 1_000_000).toFixed(1).replace(".", ",")}M`;
}

export interface UsageLike {
  inputTokens: number | null;
  outputTokens: number | null;
}

/** Suma el uso de tokens de una lista de mensajes. */
export function totalUsage(messages: UsageLike[]): { input: number; output: number } {
  return messages.reduce(
    (acc, m) => ({ input: acc.input + (m.inputTokens ?? 0), output: acc.output + (m.outputTokens ?? 0) }),
    { input: 0, output: 0 },
  );
}

/** "compatible::abliterated-model" ↔ { provider, model } para los <select>. */
export function encodeModel(provider: string, model: string): string {
  return `${provider}::${model}`;
}

export function decodeModel(value: string): { provider: string; model: string } | null {
  const i = value.indexOf("::");
  if (i <= 0 || i === value.length - 2) return null;
  return { provider: value.slice(0, i), model: value.slice(i + 2) };
}

/** Nombre de archivo seguro para exportar una conversación. */
export function exportFileName(title: string): string {
  const base = title
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[<>:"/\\|?*\u0000-\u001f]/g, "")
    .replace(/\s+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 60);
  return `${base || "conversacion"}.md`;
}
