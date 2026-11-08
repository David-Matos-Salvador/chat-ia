import { useEffect, useState } from "react";
import { useChat, type Theme } from "../store/chat";
import { api, type AppInfo } from "../lib/api";
import { Dialog } from "./Dialog";
import { KeysPanel } from "./KeysPanel";

const SHORTCUTS: [string, string][] = [
  ["Enter", "Enviar mensaje"],
  ["Shift + Enter", "Salto de línea"],
  ["Esc", "Detener la respuesta"],
  ["Ctrl + N", "Nuevo chat"],
  ["Ctrl + K", "Buscar conversaciones"],
  ["Ctrl + ,", "Abrir Ajustes"],
];

export function SettingsDialog() {
  const open = useChat((s) => s.settingsOpen);
  const setOpen = useChat((s) => s.setSettingsOpen);
  const theme = useChat((s) => s.theme);
  const setTheme = useChat((s) => s.setTheme);
  const refreshProviders = useChat((s) => s.refreshProviders);
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    if (open) api.appInfo().then(setInfo).catch(() => {});
  }, [open]);

  function close() {
    setOpen(false);
    // Las keys o la URL del proveedor pudieron cambiar: recargar modelos.
    void refreshProviders();
  }

  return (
    <Dialog open={open} onClose={close} title="Ajustes" wide>
      <section aria-labelledby="theme-title">
        <h3 id="theme-title" className="font-display text-lg text-text">
          Apariencia
        </h3>
        <div role="radiogroup" aria-labelledby="theme-title" className="mt-3 flex gap-2">
          {(
            [
              ["dark", "Oscuro futurista"],
              ["light", "Claro"],
            ] as [Theme, string][]
          ).map(([value, label]) => (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={theme === value}
              onClick={() => void setTheme(value)}
              className={`rounded-md border px-3 py-2 text-sm ${
                theme === value ? "border-accent bg-accent/15 text-accent" : "border-line text-text hover:border-accent"
              }`}
            >
              {label}
            </button>
          ))}
        </div>
      </section>

      <KeysPanel />

      <section aria-labelledby="shortcuts-title" className="mt-8 border-t border-line pt-6">
        <h3 id="shortcuts-title" className="font-display text-lg text-text">
          Atajos de teclado
        </h3>
        <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-sm">
          {SHORTCUTS.map(([keys, action]) => (
            <div key={keys} className="contents">
              <dt>
                <kbd className="rounded border border-line bg-panel-2 px-1.5 py-0.5 font-mono text-xs text-text">{keys}</kbd>
              </dt>
              <dd className="text-muted">{action}</dd>
            </div>
          ))}
        </dl>
      </section>

      <p className="mt-8 border-t border-line pt-4 font-mono text-xs text-muted">
        {info ? `${info.name} v${info.version}` : "Chat IA"} · tus datos se guardan solo en este equipo
      </p>
    </Dialog>
  );
}
