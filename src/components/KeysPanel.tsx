import { useEffect, useId, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, PROVIDERS, type KeyStatus, type Provider } from "../lib/api";

type Feedback = { tone: "ok" | "warn" | "error"; text: string } | null;

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/**
 * Gestión de API keys (fase 3).
 * La key se escribe aquí, se envía una vez al backend y se borra del campo.
 * El backend solo devuelve si está configurada y una versión enmascarada.
 */
export function KeysPanel() {
  const [statuses, setStatuses] = useState<Record<Provider, KeyStatus | undefined>>({
    anthropic: undefined,
    openai: undefined,
  });
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    api.keys
      .statusAll()
      .then((all) => {
        const next = { anthropic: undefined, openai: undefined } as Record<Provider, KeyStatus | undefined>;
        for (const s of all) next[s.provider] = s;
        setStatuses(next);
      })
      .catch((err) => setLoadError(errorText(err)));
  }, []);

  const update = (s: KeyStatus) => setStatuses((prev) => ({ ...prev, [s.provider]: s }));

  return (
    <section aria-labelledby="keys-title" className="mt-8 border-t border-line pt-6">
      <h3 id="keys-title" className="font-display text-lg text-text">
        API keys
      </h3>
      <p className="mt-1 text-sm text-muted">
        Se guardan cifradas en el Administrador de credenciales de Windows. La app nunca las muestra completas.
      </p>

      {loadError && (
        <p role="alert" className="mt-4 rounded-md border border-danger/60 bg-danger/10 p-3 text-sm text-danger">
          No se pudo leer el estado de las keys: {loadError}
        </p>
      )}

      <div className="mt-4 space-y-4">
        {PROVIDERS.map((p) => (
          <ProviderKeyRow key={p.id} provider={p} status={statuses[p.id]} onChange={update} />
        ))}
      </div>
    </section>
  );
}

function ProviderKeyRow({
  provider,
  status,
  onChange,
}: {
  provider: (typeof PROVIDERS)[number];
  status: KeyStatus | undefined;
  onChange: (s: KeyStatus) => void;
}) {
  const inputId = useId();
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState<"save" | "test" | "delete" | null>(null);
  const [feedback, setFeedback] = useState<Feedback>(null);

  const configured = status?.configured ?? false;

  async function save() {
    setBusy("save");
    setFeedback(null);
    try {
      const next = await api.keys.save(provider.id, value);
      onChange(next);
      setValue("");
      // Probar justo después de guardar ahorra un clic y detecta keys mal copiadas.
      setBusy("test");
      const check = await api.keys.test(provider.id);
      setFeedback({
        tone: check.ok ? (check.status === "valid" ? "ok" : "warn") : "error",
        text: check.ok ? `Key guardada. ${check.message}` : `Key guardada, pero: ${check.message}`,
      });
    } catch (err) {
      setFeedback({ tone: "error", text: errorText(err) });
    } finally {
      setBusy(null);
    }
  }

  async function test() {
    setBusy("test");
    setFeedback(null);
    try {
      const check = await api.keys.test(provider.id);
      setFeedback({ tone: check.ok ? (check.status === "valid" ? "ok" : "warn") : "error", text: check.message });
    } catch (err) {
      setFeedback({ tone: "error", text: errorText(err) });
    } finally {
      setBusy(null);
    }
  }

  async function remove() {
    const confirmed = await ask(`¿Borrar la API key de ${provider.label}? Tendrás que pegarla de nuevo para usarla.`, {
      title: "Borrar API key",
      kind: "warning",
      okLabel: "Borrar",
      cancelLabel: "Cancelar",
    });
    if (!confirmed) return;
    setBusy("delete");
    setFeedback(null);
    try {
      onChange(await api.keys.remove(provider.id));
      setFeedback({ tone: "ok", text: "Key borrada." });
    } catch (err) {
      setFeedback({ tone: "error", text: errorText(err) });
    } finally {
      setBusy(null);
    }
  }

  const toneClass = {
    ok: "border-ok/60 bg-ok/10 text-ok",
    warn: "border-accent-2/60 bg-accent-2/10 text-accent-2",
    error: "border-danger/60 bg-danger/10 text-danger",
  };

  return (
    <div className="rounded-md border border-line bg-panel-2 p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <label htmlFor={inputId} className="font-medium text-text">
          {provider.label}
        </label>
        <span className="flex items-center gap-2 font-mono text-xs text-muted">
          <span className={`h-2 w-2 rounded-full ${configured ? "bg-ok" : "bg-muted"}`} aria-hidden="true" />
          {status === undefined ? "Cargando…" : configured ? `Configurada · ${status.masked}` : "Sin configurar"}
        </span>
      </div>

      <form
        className="mt-3 flex flex-wrap gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (value.trim()) void save();
        }}
      >
        <input
          id={inputId}
          type="password"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder={configured ? "Pega una key nueva para reemplazarla" : provider.hint}
          autoComplete="off"
          spellCheck={false}
          disabled={busy !== null}
          className="min-w-0 flex-1 rounded-md border border-line bg-bg px-3 py-2 font-mono text-sm text-text placeholder:text-muted/70 focus:border-accent"
        />
        <button
          type="submit"
          disabled={!value.trim() || busy !== null}
          className="rounded-md border border-accent/60 bg-accent/10 px-3 py-2 text-sm text-accent hover:bg-accent/20 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {busy === "save" ? "Guardando…" : "Guardar"}
        </button>
        <button
          type="button"
          onClick={test}
          disabled={!configured || busy !== null}
          className="rounded-md border border-line px-3 py-2 text-sm text-text hover:border-accent disabled:cursor-not-allowed disabled:opacity-50"
        >
          {busy === "test" ? "Probando…" : "Probar"}
        </button>
        <button
          type="button"
          onClick={remove}
          disabled={!configured || busy !== null}
          className="rounded-md border border-line px-3 py-2 text-sm text-danger hover:border-danger disabled:cursor-not-allowed disabled:opacity-50"
        >
          {busy === "delete" ? "Borrando…" : "Borrar"}
        </button>
      </form>

      <div aria-live="polite">
        {feedback && <p className={`mt-3 rounded-md border p-2 text-sm ${toneClass[feedback.tone]}`}>{feedback.text}</p>}
      </div>

      <button
        type="button"
        onClick={() => void openUrl(provider.consoleUrl)}
        className="mt-2 text-xs text-muted underline decoration-dotted underline-offset-2 hover:text-accent"
      >
        ¿No tienes una key? Créala en la consola de {provider.label.split(" ")[0]}
      </button>
    </div>
  );
}
