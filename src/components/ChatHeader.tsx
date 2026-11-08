import { useId } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { useChat, useCurrentModel } from "../store/chat";
import { decodeModel, encodeModel, exportFileName, formatTokens, totalUsage } from "../lib/format";
import type { Provider } from "../lib/api";
import { IconDownload, IconSliders } from "./icons";

const PROVIDER_ORDER: Provider[] = ["compatible", "anthropic", "openai"];

/** Selector de proveedor y modelo, agrupado por proveedor con key configurada. */
export function ModelPicker() {
  const id = useId();
  const keys = useChat((s) => s.keys);
  const models = useChat((s) => s.models);
  const modelErrors = useChat((s) => s.modelErrors);
  const compatible = useChat((s) => s.compatible);
  const streaming = useChat((s) => s.streaming !== null);
  const chooseModel = useChat((s) => s.chooseModel);
  const current = useCurrentModel();

  const providers = PROVIDER_ORDER.filter((p) => keys.some((k) => k.provider === p && k.configured));
  const label = (p: Provider) =>
    p === "anthropic" ? "Claude (Anthropic)" : p === "openai" ? "OpenAI" : compatible?.name ?? "Compatible con OpenAI";

  if (providers.length === 0) {
    return <span className="font-mono text-xs text-muted">Sin API keys · abre Ajustes</span>;
  }

  const value = current ? encodeModel(current.provider, current.model) : "";

  return (
    <div className="flex items-center gap-2">
      <label htmlFor={id} className="font-mono text-xs uppercase tracking-widest text-muted">
        Modelo
      </label>
      <select
        id={id}
        value={value}
        disabled={streaming}
        onChange={(e) => {
          const choice = decodeModel(e.target.value);
          if (choice) void chooseModel({ provider: choice.provider as Provider, model: choice.model });
        }}
        className="max-w-[18rem] rounded-md border border-line bg-bg px-2 py-1.5 font-mono text-sm text-text focus:border-accent disabled:opacity-60"
      >
        {!current && <option value="">Elige un modelo…</option>}
        {providers.map((p) => {
          const list = models[p] ?? [];
          const hasCurrent = current?.provider === p && list.some((m) => m.id === current.model);
          return (
            <optgroup key={p} label={label(p)}>
              {current?.provider === p && !hasCurrent && (
                <option value={encodeModel(p, current.model)}>{current.model}</option>
              )}
              {list.map((m) => (
                <option key={m.id} value={encodeModel(p, m.id)}>
                  {m.name === m.id ? m.id : `${m.name}`}
                </option>
              ))}
              {list.length === 0 && (
                <option disabled value="">
                  {modelErrors[p] ? "No se pudo cargar la lista" : "Cargando modelos…"}
                </option>
              )}
            </optgroup>
          );
        })}
      </select>
    </div>
  );
}

export function ChatHeader() {
  const conversation = useChat((s) => s.conversations.find((c) => c.id === s.activeId));
  const messages = useChat((s) => s.messages);
  const streaming = useChat((s) => s.streaming !== null);
  const setPromptOpen = useChat((s) => s.setPromptOpen);
  const exportTo = useChat((s) => s.exportTo);
  const usage = totalUsage(messages);

  async function exportConversation() {
    if (!conversation) return;
    const path = await save({
      title: "Exportar conversación",
      defaultPath: exportFileName(conversation.title),
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (path) await exportTo(path);
  }

  return (
    <header className="flex flex-wrap items-center justify-between gap-3 border-b border-line bg-panel/80 px-5 py-2.5 backdrop-blur">
      <div className="min-w-[10rem] flex-1">
        <h1 className="truncate text-sm font-semibold text-text">{conversation?.title ?? "Nuevo chat"}</h1>
        <p className="truncate font-mono text-[0.6875rem] text-muted" aria-label="Uso de tokens de esta conversación">
          {usage.input + usage.output > 0
            ? `tokens · entrada ${formatTokens(usage.input)} · salida ${formatTokens(usage.output)}`
            : conversation?.systemPrompt
              ? "con instrucciones personalizadas"
              : "listo"}
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <ModelPicker />
        <button
          type="button"
          onClick={() => setPromptOpen(true)}
          disabled={!conversation || streaming}
          title="Instrucciones del sistema para este chat"
          className={`flex items-center gap-1.5 rounded-md border px-2.5 py-1.5 text-sm hover:border-accent disabled:cursor-not-allowed disabled:opacity-50 ${
            conversation?.systemPrompt ? "border-accent/60 text-accent" : "border-line text-text"
          }`}
        >
          <IconSliders /> Instrucciones
        </button>
        <button
          type="button"
          onClick={() => void exportConversation()}
          disabled={!conversation || messages.length === 0}
          title="Exportar a Markdown"
          aria-label="Exportar a Markdown"
          className="rounded-md border border-line p-1.5 text-text hover:border-accent disabled:cursor-not-allowed disabled:opacity-50"
        >
          <IconDownload />
        </button>
      </div>
    </header>
  );
}
