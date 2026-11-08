import { useEffect, useRef } from "react";
import { useChat, useCurrentModel } from "../store/chat";
import { IconSend, IconStop } from "./icons";

export const COMPOSER_ID = "composer-input";
const MAX_HEIGHT = 220;

/** Caja de texto: Enter envía, Shift+Enter hace salto de línea, crece con el contenido. */
export function Composer({ draft, setDraft }: { draft: string; setDraft: (v: string) => void }) {
  const ref = useRef<HTMLTextAreaElement>(null);
  const streaming = useChat((s) => s.streaming !== null);
  const send = useChat((s) => s.send);
  const stop = useChat((s) => s.stop);
  const hasKeys = useChat((s) => s.keys.some((k) => k.configured));
  const current = useCurrentModel();

  const canSend = draft.trim().length > 0 && !streaming && hasKeys && current !== null;

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    // Vacío: altura natural de una línea (el placeholder no debe estirar la caja).
    if (!draft) {
      el.style.height = "";
      return;
    }
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, MAX_HEIGHT)}px`;
  }, [draft]);

  function submit() {
    if (!canSend) return;
    const text = draft;
    setDraft("");
    void send(text);
  }

  const placeholder = !hasKeys
    ? "Añade una API key en Ajustes para empezar"
    : !current
      ? "Elige un modelo arriba"
      : "Escribe un mensaje… (Enter para enviar, Shift+Enter para salto de línea)";

  return (
    <div className="border-t border-line bg-panel/80 px-5 py-3 backdrop-blur">
      <form
        className="mx-auto flex w-full max-w-3xl items-end gap-2 rounded-lg border border-line bg-bg p-2 focus-within:border-accent"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <label htmlFor={COMPOSER_ID} className="sr-only">
          Mensaje
        </label>
        <textarea
          id={COMPOSER_ID}
          ref={ref}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            // isComposing: no enviar mientras se escribe con un método de entrada (acentos, IME).
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit();
            }
            if (e.key === "Escape" && streaming) void stop();
          }}
          rows={1}
          placeholder={placeholder}
          disabled={!hasKeys}
          className="h-10 max-h-[220px] min-h-[2.5rem] flex-1 resize-none bg-transparent px-2 py-2 text-text placeholder:truncate placeholder:text-muted/80 focus:outline-none disabled:cursor-not-allowed"
        />
        {streaming ? (
          <button
            type="button"
            onClick={() => void stop()}
            className="flex h-10 items-center gap-1.5 rounded-md border border-danger/60 bg-danger/10 px-3 text-sm text-danger hover:bg-danger/20"
            title="Detener (Esc)"
          >
            <IconStop /> Detener
          </button>
        ) : (
          <button
            type="submit"
            disabled={!canSend}
            className="flex h-10 items-center gap-1.5 rounded-md border border-accent/60 bg-accent/15 px-3 text-sm font-medium text-accent hover:bg-accent/25 disabled:cursor-not-allowed disabled:opacity-40"
            title="Enviar (Enter)"
          >
            <IconSend /> Enviar
          </button>
        )}
      </form>
      <p className="mx-auto mt-1.5 max-w-3xl text-center text-[0.6875rem] text-muted">
        La IA puede equivocarse. Verifica la información importante.
      </p>
    </div>
  );
}
