import { useEffect, useRef, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { useChat } from "../store/chat";
import { groupByDate } from "../lib/format";
import type { Conversation } from "../lib/api";
import { IconEdit, IconPlus, IconSearch, IconSettings, IconTrash } from "./icons";

export const SEARCH_INPUT_ID = "conversation-search";

export function Sidebar() {
  const conversations = useChat((s) => s.conversations);
  const query = useChat((s) => s.query);
  const setQuery = useChat((s) => s.setQuery);
  const loadConversations = useChat((s) => s.loadConversations);
  const newChat = useChat((s) => s.newChat);
  const setSettingsOpen = useChat((s) => s.setSettingsOpen);

  // Buscar mientras se escribe, con una pequeña espera para no consultar en cada tecla.
  useEffect(() => {
    const t = setTimeout(() => void loadConversations(), 200);
    return () => clearTimeout(t);
  }, [query, loadConversations]);

  const groups = groupByDate(conversations);

  return (
    <aside aria-label="Conversaciones" className="flex h-full w-72 shrink-0 flex-col border-r border-line bg-panel">
      <div className="flex items-center justify-between px-4 pb-3 pt-4">
        <span className="font-display text-base tracking-[0.25em] text-accent">CHAT IA</span>
      </div>

      <div className="space-y-2 px-3">
        <button
          type="button"
          onClick={newChat}
          className="flex w-full items-center justify-center gap-2 rounded-md border border-accent/60 bg-accent/10 px-3 py-2 text-sm font-medium text-accent hover:bg-accent/20"
          title="Nuevo chat (Ctrl+N)"
        >
          <IconPlus /> Nuevo chat
        </button>
        <div className="relative">
          <span className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted">
            <IconSearch />
          </span>
          <label htmlFor={SEARCH_INPUT_ID} className="sr-only">
            Buscar conversaciones
          </label>
          <input
            id={SEARCH_INPUT_ID}
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Buscar (Ctrl+K)"
            className="w-full rounded-md border border-line bg-bg py-2 pl-9 pr-3 text-sm text-text placeholder:text-muted/80 focus:border-accent"
          />
        </div>
      </div>

      <nav className="mt-3 flex-1 overflow-y-auto px-2 pb-3" aria-label="Historial">
        {groups.length === 0 && (
          <p className="px-3 py-6 text-center text-sm text-muted">
            {query.trim() ? "No hay conversaciones que coincidan." : "Aún no hay conversaciones."}
          </p>
        )}
        {groups.map((group) => (
          <section key={group.label} className="mt-3 first:mt-0">
            <h3 className="px-3 pb-1 font-mono text-[11px] uppercase tracking-widest text-muted">{group.label}</h3>
            <ul>
              {group.items.map((c) => (
                <ConversationItem key={c.id} conversation={c} />
              ))}
            </ul>
          </section>
        ))}
      </nav>

      <div className="border-t border-line p-3">
        <button
          type="button"
          onClick={() => setSettingsOpen(true)}
          className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-sm text-text hover:bg-panel-2"
          title="Ajustes (Ctrl+,)"
        >
          <IconSettings /> Ajustes
        </button>
      </div>
    </aside>
  );
}

function ConversationItem({ conversation }: { conversation: Conversation }) {
  const active = useChat((s) => s.activeId === conversation.id);
  const streaming = useChat((s) => s.streaming?.conversationId === conversation.id);
  const select = useChat((s) => s.select);
  const rename = useChat((s) => s.rename);
  const remove = useChat((s) => s.remove);
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState(conversation.title);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  async function commit() {
    setEditing(false);
    const next = title.trim();
    if (next && next !== conversation.title) await rename(conversation.id, next);
    else setTitle(conversation.title);
  }

  async function confirmDelete() {
    const ok = await ask(`¿Borrar «${conversation.title}»? Se borrarán todos sus mensajes.`, {
      title: "Borrar conversación",
      kind: "warning",
      okLabel: "Borrar",
      cancelLabel: "Cancelar",
    });
    if (ok) await remove(conversation.id);
  }

  if (editing) {
    return (
      <li className="px-1 py-0.5">
        <label htmlFor={`rename-${conversation.id}`} className="sr-only">
          Nuevo título
        </label>
        <input
          id={`rename-${conversation.id}`}
          ref={inputRef}
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onBlur={() => void commit()}
          onKeyDown={(e) => {
            if (e.key === "Enter") void commit();
            if (e.key === "Escape") {
              setTitle(conversation.title);
              setEditing(false);
            }
          }}
          className="w-full rounded-md border border-accent bg-bg px-2 py-1.5 text-sm text-text"
        />
      </li>
    );
  }

  return (
    <li className="group relative px-1 py-0.5">
      <button
        type="button"
        onClick={() => void select(conversation.id)}
        onDoubleClick={() => setEditing(true)}
        aria-current={active ? "page" : undefined}
        className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 pr-16 text-left text-sm ${
          active ? "bg-accent/15 text-text ring-1 ring-accent/40" : "text-text/90 hover:bg-panel-2"
        }`}
      >
        {streaming && <span className="h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-accent" aria-label="Respondiendo" />}
        <span className="truncate">{conversation.title}</span>
      </button>
      <div className="absolute right-2 top-1/2 flex -translate-y-1/2 gap-0.5 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100">
        <button
          type="button"
          onClick={() => setEditing(true)}
          aria-label={`Renombrar ${conversation.title}`}
          className="rounded p-1 text-muted hover:bg-bg hover:text-accent"
        >
          <IconEdit />
        </button>
        <button
          type="button"
          onClick={() => void confirmDelete()}
          aria-label={`Borrar ${conversation.title}`}
          className="rounded p-1 text-muted hover:bg-bg hover:text-danger"
        >
          <IconTrash />
        </button>
      </div>
    </li>
  );
}
