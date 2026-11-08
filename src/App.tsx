import { useCallback, useEffect, useState } from "react";
import { api, type AppInfo, type DbStatus } from "./lib/api";
import { KeysPanel } from "./components/KeysPanel";

type Status =
  | { kind: "checking" }
  | { kind: "ok"; info: AppInfo; db: DbStatus }
  | { kind: "error"; message: string };

/** Modelo por defecto para el chat de prueba. Se podrá elegir en la fase 6. */
const TEST_MODEL = "claude-sonnet-5-5";

/**
 * Pantalla base (fases 1 a 3).
 * Confirma que el frontend habla con el backend Rust, que la base de datos funciona
 * y permite gestionar las API keys.
 * La interfaz completa del chat llega en la fase 6.
 */
export default function App() {
  const [status, setStatus] = useState<Status>({ kind: "checking" });
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [info, db] = await Promise.all([api.appInfo(), api.dbStatus()]);
      setStatus({ kind: "ok", info, db });
    } catch (err) {
      setStatus({ kind: "error", message: err instanceof Error ? err.message : String(err) });
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function createTestChat() {
    setBusy(true);
    try {
      await api.conversations.create(TEST_MODEL, "Chat de prueba");
      await refresh();
    } catch (err) {
      setStatus({ kind: "error", message: err instanceof Error ? err.message : String(err) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="bg-grid flex h-full flex-col">
      <header className="flex items-center justify-between border-b border-line bg-panel/80 px-6 py-3">
        <h1 className="font-display text-lg tracking-widest text-accent">CHAT IA</h1>
        <StatusBadge status={status} />
      </header>

      <main className="flex flex-1 justify-center overflow-y-auto p-6">
        <section className="my-auto w-full max-w-xl rounded-lg border border-line bg-panel p-8">
          <p className="font-mono text-xs uppercase tracking-widest text-muted">Fase 3 · API keys</p>
          <h2 className="mt-2 font-display text-2xl text-text">Sistema listo</h2>
          <p className="mt-4 text-muted">
            El backend Rust guarda las conversaciones en una base de datos SQLite local. En las próximas
            fases se añaden las API keys, el streaming y la interfaz completa del chat.
          </p>

          <dl className="mt-6 grid grid-cols-[auto_1fr] gap-x-6 gap-y-3 font-mono text-sm">
            <dt className="text-muted">Backend</dt>
            <dd className="text-text">
              {status.kind === "ok" ? `Rust · ${status.info.name} v${status.info.version}` : "—"}
            </dd>
            <dt className="text-muted">Esquema</dt>
            <dd className="text-text">{status.kind === "ok" ? `v${status.db.schemaVersion}` : "—"}</dd>
            <dt className="text-muted">Conversaciones</dt>
            <dd className="text-text">{status.kind === "ok" ? status.db.conversations : "—"}</dd>
            <dt className="text-muted">Mensajes</dt>
            <dd className="text-text">{status.kind === "ok" ? status.db.messages : "—"}</dd>
            <dt className="text-muted">Archivo</dt>
            <dd className="break-all text-text">{status.kind === "ok" ? status.db.path : "—"}</dd>
          </dl>

          <button
            type="button"
            onClick={createTestChat}
            disabled={status.kind !== "ok" || busy}
            className="mt-6 rounded-md border border-accent/60 bg-accent/10 px-4 py-2 font-mono text-sm text-accent transition-colors hover:bg-accent/20 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy ? "Creando…" : "Crear chat de prueba"}
          </button>

          {status.kind === "error" && (
            <p role="alert" className="mt-6 rounded-md border border-danger/60 bg-danger/10 p-3 text-sm text-danger">
              No se pudo conectar con el backend: {status.message}
            </p>
          )}

          {status.kind === "ok" && <KeysPanel />}
        </section>
      </main>
    </div>
  );
}

function StatusBadge({ status }: { status: Status }) {
  const map = {
    checking: { label: "Conectando…", dot: "bg-muted" },
    ok: { label: "Backend conectado", dot: "bg-ok" },
    error: { label: "Sin conexión", dot: "bg-danger" },
  } as const;
  const { label, dot } = map[status.kind];

  return (
    <span aria-live="polite" className="flex items-center gap-2 font-mono text-xs text-muted">
      <span className={`h-2 w-2 rounded-full ${dot}`} aria-hidden="true" />
      {label}
    </span>
  );
}
