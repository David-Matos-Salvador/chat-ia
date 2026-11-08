import { useEffect, useState } from "react";
import { useChat } from "./store/chat";
import { Sidebar, SEARCH_INPUT_ID } from "./components/Sidebar";
import { ChatHeader } from "./components/ChatHeader";
import { MessageList } from "./components/MessageList";
import { Composer, COMPOSER_ID } from "./components/Composer";
import { SettingsDialog } from "./components/SettingsDialog";
import { SystemPromptDialog } from "./components/SystemPromptDialog";

function focusById(id: string) {
  const el = document.getElementById(id);
  if (el instanceof HTMLInputElement) {
    el.focus();
    el.select();
  } else if (el instanceof HTMLTextAreaElement) {
    el.focus();
  }
}

export default function App() {
  const ready = useChat((s) => s.ready);
  const init = useChat((s) => s.init);
  const newChat = useChat((s) => s.newChat);
  const activeId = useChat((s) => s.activeId);
  const setSettingsOpen = useChat((s) => s.setSettingsOpen);
  const hasKeys = useChat((s) => s.keys.some((k) => k.configured));
  const [draft, setDraft] = useState("");

  useEffect(() => {
    void init();
  }, [init]);

  // Si al arrancar no hay ninguna key, abrir Ajustes directamente.
  useEffect(() => {
    if (ready && !hasKeys) setSettingsOpen(true);
  }, [ready, hasKeys, setSettingsOpen]);

  // Al cambiar de conversación, llevar el foco a la caja de texto.
  useEffect(() => {
    if (ready) focusById(COMPOSER_ID);
  }, [activeId, ready]);

  // Atajos de teclado globales.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!e.ctrlKey || e.altKey) return;
      const key = e.key.toLowerCase();
      if (key === "n") {
        e.preventDefault();
        newChat();
        setDraft("");
        requestAnimationFrame(() => focusById(COMPOSER_ID));
      } else if (key === "k") {
        e.preventDefault();
        focusById(SEARCH_INPUT_ID);
      } else if (key === ",") {
        e.preventDefault();
        setSettingsOpen(true);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [newChat, setSettingsOpen]);

  if (!ready) {
    return (
      <div className="bg-grid flex h-full items-center justify-center">
        <p className="font-display tracking-[0.3em] text-accent" role="status">
          CARGANDO…
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full overflow-hidden">
      <Sidebar />
      <main className="bg-grid flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <ChatHeader />
        <MessageList
          onSuggestion={(text) => {
            setDraft(text);
            requestAnimationFrame(() => focusById(COMPOSER_ID));
          }}
        />
        <Composer draft={draft} setDraft={setDraft} />
      </main>
      <SettingsDialog />
      <SystemPromptDialog />
    </div>
  );
}
