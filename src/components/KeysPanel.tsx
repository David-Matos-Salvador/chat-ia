import { useEffect, useId, useState } from "react";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  api,
  COMPATIBLE_PRESETS,
  expectedKeyPrefix,
  misplacedKeyHint,
  PROVIDERS,
  type CompatibleConfig,
  type KeyCheck,
  type KeyStatus,
  type Provider,
  type ProviderInfo,
} from "../lib/api";

type Feedback = { tone: "ok" | "warn" | "error"; text: string } | null;

const TONE_CLASS = {
  ok: "border-ok/60 bg-ok/10 text-ok",
  warn: "border-accent-2/60 bg-accent-2/10 text-accent-2",
  error: "border-danger/60 bg-danger/10 text-danger",
} as const;

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function toneOf(check: KeyCheck): "ok" | "warn" | "error" {
  if (!check.ok) return "error";
  return check.status === "valid" ? "ok" : "warn";
}

/**
 * Gestión de API keys (fase 3).
 * La key se escribe aquí, se envía una vez al backend y se borra del campo.
 * El backend solo devuelve si está configurada y una versión enmascarada.
 */
export function KeysPanel() {
  const [statuses, setStatuses] = useState<Partial<Record<Provider, KeyStatus>>>({});
  const [compatible, setCompatible] = useState<CompatibleConfig | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([api.keys.statusAll(), api.compatible.get()])
      .then(([all, config]) => {
        const next: Partial<Record<Provider, KeyStatus>> = {};
        for (const s of all) next[s.provider] = s;
        setStatuses(next);
        setCompatible(config);
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
          <ProviderKeyRow
            key={p.id}
            provider={p}
            status={statuses[p.id]}
            onChange={update}
            compatible={p.id === "compatible" ? { config: compatible, onSaved: setCompatible } : undefined}
          />
        ))}
      </div>
    </section>
  );
}

function ProviderKeyRow({
  provider,
  status,
  onChange,
  compatible,
}: {
  provider: ProviderInfo;
  status: KeyStatus | undefined;
  onChange: (s: KeyStatus) => void;
  compatible?: { config: CompatibleConfig | null; onSaved: (c: CompatibleConfig) => void };
}) {
  const inputId = useId();
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState<"save" | "test" | "delete" | null>(null);
  const [feedback, setFeedback] = useState<Feedback>(null);

  const configured = status?.configured ?? false;
  // El proveedor compatible necesita su URL base antes de poder probar la key.
  const needsConfig = compatible !== undefined && compatible.config === null;
  const title = compatible?.config ? `${provider.label} · ${compatible.config.name}` : provider.label;

  async function runTest(prefix = "") {
    setBusy("test");
    const check = await api.keys.test(provider.id);
    setFeedback({ tone: toneOf(check), text: `${prefix}${check.message}` });
  }

  async function save() {
    // Una key de otro proveedor nunca funcionará aquí: no se guarda y se indica la fila correcta.
    const misplaced = misplacedKeyHint(provider.id, value);
    if (misplaced) {
      await message(misplaced, { title: "Fila equivocada", kind: "warning" });
      return;
    }

    // Aviso si lo pegado no parece una key de este servicio (por ejemplo, otro valor de la consola).
    const expected = expectedKeyPrefix(provider.id, compatible?.config ?? null);
    if (expected && !value.trim().startsWith(expected.prefix)) {
      const proceed = await ask(
        `Las keys de ${expected.service} empiezan por «${expected.prefix}», pero lo que pegaste no.\n\n` +
          `Puede que hayas copiado otro valor. En la consola del proveedor, la key completa solo se ` +
          `muestra una vez, al crearla; si ya no la tienes, crea una nueva y cópiala en ese momento.\n\n` +
          `¿Guardarla igualmente?`,
        { title: "¿Es la key correcta?", kind: "warning", okLabel: "Guardar igualmente", cancelLabel: "Cancelar" },
      );
      if (!proceed) return;
    }

    setBusy("save");
    setFeedback(null);
    try {
      onChange(await api.keys.save(provider.id, value));
      setValue("");
      if (needsConfig) {
        setFeedback({ tone: "warn", text: "Key guardada. Configura la URL base para poder probarla." });
      } else {
        // Probar justo después de guardar detecta keys mal copiadas sin otro clic.
        await runTest("Key guardada. ");
      }
    } catch (err) {
      setFeedback({ tone: "error", text: errorText(err) });
    } finally {
      setBusy(null);
    }
  }

  async function test() {
    setFeedback(null);
    try {
      await runTest();
    } catch (err) {
      setFeedback({ tone: "error", text: errorText(err) });
    } finally {
      setBusy(null);
    }
  }

  async function remove() {
    const confirmed = await ask(`¿Borrar la API key de ${title}? Tendrás que pegarla de nuevo para usarla.`, {
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

  return (
    <div className="rounded-md border border-line bg-panel-2 p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <label htmlFor={inputId} className="font-medium text-text">
          {title}
        </label>
        <span className="flex items-center gap-2 font-mono text-xs text-muted">
          <span className={`h-2 w-2 rounded-full ${configured ? "bg-ok" : "bg-muted"}`} aria-hidden="true" />
          {status === undefined ? "Cargando…" : configured ? `Configurada · ${status.masked}` : "Sin configurar"}
        </span>
      </div>

      {compatible && (
        <CompatibleConfigForm
          config={compatible.config}
          onSaved={(c) => {
            compatible.onSaved(c);
            setFeedback({
              tone: "ok",
              text: configured ? `Configuración guardada: ${c.name}. Pulsa Probar para comprobar la key.` : `Configuración guardada: ${c.name}. Ahora pega tu API key.`,
            });
          }}
          onError={(text) => setFeedback({ tone: "error", text })}
          disabled={busy !== null}
        />
      )}

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
          disabled={!configured || needsConfig || busy !== null}
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
        {feedback && <p className={`mt-3 rounded-md border p-2 text-sm ${TONE_CLASS[feedback.tone]}`}>{feedback.text}</p>}
      </div>

      {provider.consoleUrl && (
        <button
          type="button"
          onClick={() => void openUrl(provider.consoleUrl!)}
          className="mt-2 text-xs text-muted underline decoration-dotted underline-offset-2 hover:text-accent"
        >
          ¿No tienes una key? Créala en la consola de {provider.label.split(" ")[0]}
        </button>
      )}
    </div>
  );
}

/** Nombre y URL base del proveedor compatible, con atajos para servicios conocidos. */
function CompatibleConfigForm({
  config,
  onSaved,
  onError,
  disabled,
}: {
  config: CompatibleConfig | null;
  onSaved: (c: CompatibleConfig) => void;
  onError: (text: string) => void;
  disabled: boolean;
}) {
  const nameId = useId();
  const urlId = useId();
  const [name, setName] = useState(config?.name ?? "");
  const [baseUrl, setBaseUrl] = useState(config?.baseUrl ?? "");
  const [saving, setSaving] = useState(false);

  // Cuando llega la configuración guardada, rellenar los campos.
  useEffect(() => {
    if (config) {
      setName(config.name);
      setBaseUrl(config.baseUrl);
    }
  }, [config]);

  async function persist(next: CompatibleConfig) {
    setSaving(true);
    try {
      const saved = await api.compatible.set(next.name, next.baseUrl);
      setName(saved.name);
      setBaseUrl(saved.baseUrl);
      onSaved(saved);
    } catch (err) {
      onError(errorText(err));
    } finally {
      setSaving(false);
    }
  }

  const unchanged = config !== null && config.name === name.trim() && config.baseUrl === baseUrl.trim().replace(/\/+$/, "");
  const off = disabled || saving;

  return (
    <div className="mt-3 space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs text-muted">Atajos:</span>
        {COMPATIBLE_PRESETS.map((preset) => (
          <button
            key={preset.name}
            type="button"
            disabled={off}
            onClick={() => void persist(preset)}
            aria-pressed={config?.baseUrl === preset.baseUrl}
            className="rounded-md border border-line px-2 py-1 text-xs text-text hover:border-accent aria-pressed:border-accent aria-pressed:text-accent disabled:cursor-not-allowed disabled:opacity-50"
          >
            {preset.name}
          </button>
        ))}
      </div>

      <form
        className="grid grid-cols-1 gap-2 sm:grid-cols-[1fr_2fr_auto]"
        onSubmit={(e) => {
          e.preventDefault();
          void persist({ name, baseUrl });
        }}
      >
        <div>
          <label htmlFor={nameId} className="block text-xs text-muted">
            Nombre
          </label>
          <input
            id={nameId}
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Mi proveedor"
            disabled={off}
            className="mt-1 w-full rounded-md border border-line bg-bg px-3 py-2 text-sm text-text placeholder:text-muted/70 focus:border-accent"
          />
        </div>
        <div>
          <label htmlFor={urlId} className="block text-xs text-muted">
            URL base (suele terminar en /v1)
          </label>
          <input
            id={urlId}
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            placeholder="https://api.ejemplo.com/v1"
            spellCheck={false}
            disabled={off}
            className="mt-1 w-full rounded-md border border-line bg-bg px-3 py-2 font-mono text-sm text-text placeholder:text-muted/70 focus:border-accent"
          />
        </div>
        <button
          type="submit"
          disabled={off || !name.trim() || !baseUrl.trim() || unchanged}
          className="self-end rounded-md border border-accent/60 bg-accent/10 px-3 py-2 text-sm text-accent hover:bg-accent/20 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {saving ? "Guardando…" : "Guardar URL"}
        </button>
      </form>
    </div>
  );
}
