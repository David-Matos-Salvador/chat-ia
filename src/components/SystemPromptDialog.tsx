import { useEffect, useId, useState } from "react";
import { useChat } from "../store/chat";
import { Dialog } from "./Dialog";

const EXAMPLES = [
  "Responde siempre en español y de forma breve.",
  "Eres un profesor paciente: explica paso a paso y con ejemplos.",
  "Eres un revisor de código experto. Señala errores y propone mejoras concretas.",
];

/** Instrucciones del sistema para la conversación activa. */
export function SystemPromptDialog() {
  const open = useChat((s) => s.promptOpen);
  const setOpen = useChat((s) => s.setPromptOpen);
  const conversation = useChat((s) => s.conversations.find((c) => c.id === s.activeId));
  const setSystemPrompt = useChat((s) => s.setSystemPrompt);
  const [value, setValue] = useState("");
  const id = useId();

  useEffect(() => {
    if (open) setValue(conversation?.systemPrompt ?? "");
  }, [open, conversation?.systemPrompt]);

  async function save() {
    await setSystemPrompt(value.trim() ? value : null);
    setOpen(false);
  }

  return (
    <Dialog open={open} onClose={() => setOpen(false)} title="Instrucciones para este chat">
      <p className="text-sm text-muted">
        El modelo las tiene en cuenta en todas sus respuestas de esta conversación: tono, idioma, rol o formato.
      </p>
      <label htmlFor={id} className="mt-4 block text-xs text-muted">
        Instrucciones
      </label>
      <textarea
        id={id}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        rows={6}
        placeholder="Por ejemplo: Responde siempre en español y de forma breve."
        className="mt-1 w-full rounded-md border border-line bg-bg px-3 py-2 text-sm text-text placeholder:text-muted/70 focus:border-accent"
      />
      <div className="mt-2 flex flex-wrap gap-2">
        {EXAMPLES.map((e) => (
          <button
            key={e}
            type="button"
            onClick={() => setValue(e)}
            className="rounded-md border border-line px-2 py-1 text-left text-xs text-text hover:border-accent"
          >
            {e}
          </button>
        ))}
      </div>
      <div className="mt-5 flex justify-end gap-2">
        {conversation?.systemPrompt && (
          <button
            type="button"
            onClick={() => setValue("")}
            className="mr-auto rounded-md px-3 py-2 text-sm text-danger hover:bg-danger/10"
          >
            Quitar instrucciones
          </button>
        )}
        <button
          type="button"
          onClick={() => setOpen(false)}
          className="rounded-md border border-line px-3 py-2 text-sm text-text hover:border-accent"
        >
          Cancelar
        </button>
        <button
          type="button"
          onClick={() => void save()}
          className="rounded-md border border-accent/60 bg-accent/10 px-3 py-2 text-sm text-accent hover:bg-accent/20"
        >
          Guardar
        </button>
      </div>
    </Dialog>
  );
}
