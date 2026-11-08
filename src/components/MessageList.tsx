import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useChat, useCurrentModel } from "../store/chat";
import { formatTokens } from "../lib/format";
import type { Message } from "../lib/api";
import { CopyButton, Markdown } from "./Markdown";
import { IconEdit, IconFork, IconRefresh } from "./icons";

const SUGGESTIONS = [
  "Explícame un concepto difícil como si tuviera 12 años",
  "Ayúdame a redactar un correo profesional",
  "Revisa este código y sugiere mejoras",
  "Dame ideas para un proyecto de fin de semana",
];

export function MessageList({ onSuggestion }: { onSuggestion: (text: string) => void }) {
  const activeId = useChat((s) => s.activeId);
  const messages = useChat((s) => s.messages);
  const loading = useChat((s) => s.loadingMessages);
  const streaming = useChat((s) => (s.streaming?.conversationId === s.activeId ? s.streaming : null));
  const notice = useChat((s) => s.notice);
  const error = useChat((s) => s.error);
  const dismissError = useChat((s) => s.dismissError);
  const keys = useChat((s) => s.keys);
  const setSettingsOpen = useChat((s) => s.setSettingsOpen);

  const scrollRef = useRef<HTMLDivElement>(null);
  const stickToBottom = useRef(true);
  const [announcement, setAnnouncement] = useState("");

  // Solo se baja automáticamente si el usuario ya estaba al final (no le roba la posición al leer).
  function onScroll() {
    const el = scrollRef.current;
    if (!el) return;
    stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
  }

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [messages, streaming?.text, streaming?.pendingUser, notice, error]);

  useEffect(() => {
    stickToBottom.current = true;
  }, [activeId]);

  // Aviso para lectores de pantalla al terminar (anunciar cada token sería ruido).
  const wasStreaming = useRef(false);
  useEffect(() => {
    if (wasStreaming.current && !streaming) setAnnouncement("Respuesta completa.");
    if (streaming) setAnnouncement("");
    wasStreaming.current = Boolean(streaming);
  }, [streaming]);

  const lastAssistantId = [...messages].reverse().find((m) => m.role === "assistant")?.id;
  const hasKeys = keys.some((k) => k.configured);
  const empty = messages.length === 0 && !streaming && !loading;

  return (
    // min-h-0: sin esto, un hijo flex no se encoge por debajo de su contenido y una
    // respuesta larga estira toda la ventana en lugar de desplazarse solo aquí.
    // relative: los elementos absolutos de dentro (el aviso para lectores de pantalla)
    // se posicionan aquí y no al final de la página, donde la harían desplazable.
    <div ref={scrollRef} onScroll={onScroll} className="relative min-h-0 flex-1 overflow-y-auto">
      <div className="mx-auto w-full max-w-3xl px-5 py-6" aria-busy={Boolean(streaming)}>
        {empty && <EmptyState hasKeys={hasKeys} onSuggestion={onSuggestion} onOpenSettings={() => setSettingsOpen(true)} />}

        <ol className="space-y-6" aria-label="Mensajes">
          {messages.map((m) => (
            <MessageItem key={m.id} message={m} isLastAssistant={m.id === lastAssistantId} />
          ))}
          {streaming?.pendingUser && (
            <li>
              <UserBubble content={streaming.pendingUser} />
            </li>
          )}
          {streaming && (
            <li>
              <AssistantShell>
                {streaming.text ? (
                  <div className="stream-cursor">
                    <Markdown text={streaming.text} />
                  </div>
                ) : (
                  <ThinkingDots />
                )}
              </AssistantShell>
            </li>
          )}
        </ol>

        {notice && !streaming && (
          <p className="mt-4 rounded-md border border-accent-2/50 bg-accent-2/10 px-3 py-2 text-sm text-accent-2">{notice}</p>
        )}
        {error && (
          <div role="alert" className="mt-4 flex items-start justify-between gap-3 rounded-md border border-danger/60 bg-danger/10 px-3 py-2 text-sm text-danger">
            <span>{error}</span>
            <button type="button" onClick={dismissError} className="shrink-0 underline">
              Cerrar
            </button>
          </div>
        )}
        <p className="sr-only" aria-live="polite">
          {announcement}
        </p>
      </div>
    </div>
  );
}

function EmptyState({
  hasKeys,
  onSuggestion,
  onOpenSettings,
}: {
  hasKeys: boolean;
  onSuggestion: (text: string) => void;
  onOpenSettings: () => void;
}) {
  const current = useCurrentModel();
  return (
    <div className="flex flex-col items-center py-16 text-center">
      <p className="font-display text-3xl tracking-[0.3em] text-accent">CHAT IA</p>
      <p className="mt-3 text-muted">
        {hasKeys
          ? current
            ? `Escribe un mensaje para empezar · ${current.model}`
            : "Elige un modelo arriba para empezar."
          : "Para empezar, añade una API key."}
      </p>
      {!hasKeys ? (
        <button
          type="button"
          onClick={onOpenSettings}
          className="mt-6 rounded-md border border-accent/60 bg-accent/10 px-4 py-2 text-sm text-accent hover:bg-accent/20"
        >
          Abrir Ajustes
        </button>
      ) : (
        <div className="mt-8 grid w-full max-w-xl grid-cols-1 gap-2 sm:grid-cols-2">
          {SUGGESTIONS.map((s) => (
            <button
              key={s}
              type="button"
              onClick={() => onSuggestion(s)}
              className="rounded-md border border-line bg-panel px-3 py-2.5 text-left text-sm text-text hover:border-accent"
            >
              {s}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function ThinkingDots() {
  return (
    <span className="inline-flex items-center gap-1 py-2" aria-label="Pensando">
      {[0, 1, 2].map((i) => (
        <span key={i} className="h-1.5 w-1.5 animate-pulse rounded-full bg-accent" style={{ animationDelay: `${i * 150}ms` }} />
      ))}
    </span>
  );
}

function UserBubble({ content }: { content: string }) {
  return (
    <div className="flex justify-end">
      <div className="max-w-[85%] whitespace-pre-wrap rounded-lg border border-accent/30 bg-accent/10 px-4 py-2.5 text-text">
        {content}
      </div>
    </div>
  );
}

function AssistantShell({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex gap-3">
      <span className="mt-1 flex h-6 w-6 shrink-0 items-center justify-center rounded border border-accent/50 font-display text-[0.625rem] text-accent" aria-hidden="true">
        IA
      </span>
      <div className="min-w-0 flex-1 text-text">{children}</div>
    </div>
  );
}

function MessageItem({ message, isLastAssistant }: { message: Message; isLastAssistant: boolean }) {
  const streaming = useChat((s) => s.streaming !== null);
  const send = useChat((s) => s.send);
  const regenerate = useChat((s) => s.regenerate);
  const fork = useChat((s) => s.fork);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(message.content);

  if (message.role === "user") {
    if (editing) {
      return (
        <li className="flex justify-end">
          <form
            className="w-full max-w-[85%] space-y-2"
            onSubmit={(e) => {
              e.preventDefault();
              setEditing(false);
              if (draft.trim() && draft.trim() !== message.content) void send(draft, message.id);
            }}
          >
            <label htmlFor={`edit-${message.id}`} className="sr-only">
              Editar mensaje
            </label>
            <textarea
              id={`edit-${message.id}`}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              rows={Math.min(8, draft.split("\n").length + 1)}
              autoFocus
              className="w-full rounded-lg border border-accent bg-bg px-3 py-2 text-text"
            />
            <p className="text-xs text-muted">Al enviar, se borra lo que vino después y se genera una respuesta nueva.</p>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => {
                  setDraft(message.content);
                  setEditing(false);
                }}
                className="rounded-md border border-line px-3 py-1.5 text-sm text-text hover:border-accent"
              >
                Cancelar
              </button>
              <button type="submit" className="rounded-md border border-accent/60 bg-accent/10 px-3 py-1.5 text-sm text-accent hover:bg-accent/20">
                Enviar
              </button>
            </div>
          </form>
        </li>
      );
    }
    return (
      <li className="group">
        <UserBubble content={message.content} />
        <div className="mt-1 flex justify-end gap-1 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
          <CopyButton text={message.content} />
          <button
            type="button"
            onClick={() => setEditing(true)}
            disabled={streaming}
            className="flex items-center gap-1 rounded px-2 py-0.5 font-mono text-xs text-muted hover:bg-panel-2 hover:text-accent disabled:opacity-50"
          >
            <IconEdit /> Editar
          </button>
        </div>
      </li>
    );
  }

  const tokens =
    message.inputTokens != null || message.outputTokens != null
      ? `↑${formatTokens(message.inputTokens ?? 0)} ↓${formatTokens(message.outputTokens ?? 0)}`
      : null;

  return (
    <li className="group">
      <AssistantShell>
        <Markdown text={message.content} />
        <div className="mt-2 flex flex-wrap items-center gap-1 text-muted">
          <CopyButton text={message.content} />
          {isLastAssistant && (
            <button
              type="button"
              onClick={() => void regenerate()}
              disabled={streaming}
              className="flex items-center gap-1 rounded px-2 py-0.5 font-mono text-xs hover:bg-panel-2 hover:text-accent disabled:opacity-50"
            >
              <IconRefresh /> Regenerar
            </button>
          )}
          <button
            type="button"
            onClick={() => void fork(message.id)}
            disabled={streaming}
            title="Crear un chat nuevo con la conversación hasta aquí"
            className="flex items-center gap-1 rounded px-2 py-0.5 font-mono text-xs hover:bg-panel-2 hover:text-accent disabled:opacity-50"
          >
            <IconFork /> Bifurcar
          </button>
          <span className="ml-auto font-mono text-[0.6875rem]" title="Tokens de entrada ↑ y de salida ↓">
            {message.model}
            {tokens ? ` · ${tokens}` : ""}
          </span>
        </div>
      </AssistantShell>
    </li>
  );
}
