import { useMemo } from "react";
import { create } from "zustand";
import {
  api,
  DEFAULT_ANTHROPIC_MODEL,
  type CompatibleConfig,
  type Conversation,
  type KeyStatus,
  type Message,
  type ModelInfo,
  type Provider,
} from "../lib/api";

export type Theme = "dark" | "light";

const SETTING_THEME = "ui.theme";
const SETTING_DEFAULT_PROVIDER = "chat.default_provider";
const SETTING_DEFAULT_MODEL = "chat.default_model";

export interface ModelChoice {
  provider: Provider;
  model: string;
}

/** Respuesta que se está recibiendo ahora mismo. */
export interface Streaming {
  conversationId: string;
  text: string;
  /** Mensaje del usuario mientras el backend aún no confirma que lo guardó. */
  pendingUser: string | null;
  kind: "send" | "regenerate";
}

interface ChatState {
  ready: boolean;
  conversations: Conversation[];
  query: string;
  activeId: string | null;
  messages: Message[];
  loadingMessages: boolean;
  streaming: Streaming | null;
  notice: string | null;
  error: string | null;
  keys: KeyStatus[];
  compatible: CompatibleConfig | null;
  models: Partial<Record<Provider, ModelInfo[]>>;
  modelErrors: Partial<Record<Provider, string>>;
  defaults: ModelChoice | null;
  theme: Theme;
  settingsOpen: boolean;
  promptOpen: boolean;

  init: () => Promise<void>;
  refreshProviders: () => Promise<void>;
  loadModels: (provider: Provider) => Promise<void>;
  setQuery: (query: string) => void;
  loadConversations: () => Promise<void>;
  select: (id: string) => Promise<void>;
  newChat: () => void;
  chooseModel: (choice: ModelChoice) => Promise<void>;
  send: (content: string, editFrom?: string) => Promise<void>;
  regenerate: () => Promise<void>;
  stop: () => Promise<void>;
  rename: (id: string, title: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  setSystemPrompt: (prompt: string | null) => Promise<void>;
  exportTo: (path: string) => Promise<void>;
  setTheme: (theme: Theme) => Promise<void>;
  setSettingsOpen: (open: boolean) => void;
  setPromptOpen: (open: boolean) => void;
  dismissError: () => void;
}

function message(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
}

function configured(keys: KeyStatus[], provider: Provider): boolean {
  return keys.some((k) => k.provider === provider && k.configured);
}

/** Pone una conversación actualizada al principio de la lista. */
function upsertTop(list: Conversation[], conv: Conversation): Conversation[] {
  return [conv, ...list.filter((c) => c.id !== conv.id)];
}

export const useChat = create<ChatState>((set, get) => {
  /** Elige un modelo por defecto razonable si no hay uno válido guardado. */
  function resolveDefaults() {
    const { defaults, keys, models, modelErrors } = get();
    // Un modelo guardado sigue valiendo si su proveedor tiene key y su lista no falló.
    if (defaults && configured(keys, defaults.provider) && !modelErrors[defaults.provider]) return;
    let next: ModelChoice | null = null;
    if (configured(keys, "compatible") && models.compatible?.length) {
      next = { provider: "compatible", model: models.compatible[0].id };
    } else if (configured(keys, "anthropic") && !modelErrors.anthropic) {
      next = { provider: "anthropic", model: models.anthropic?.[0]?.id ?? DEFAULT_ANTHROPIC_MODEL };
    } else if (configured(keys, "openai") && models.openai?.length) {
      next = { provider: "openai", model: models.openai[0].id };
    }
    if (next) set({ defaults: next });
  }

  async function reloadMessages(conversationId: string) {
    const messages = await api.messages.list(conversationId);
    if (get().activeId === conversationId) set({ messages });
  }

  /** Común a enviar y regenerar: aplica el resultado o el error. */
  async function runStream(conversationId: string, run: Promise<import("../lib/api").ChatOutcome>) {
    try {
      const outcome = await run;
      set((s) => ({
        conversations: upsertTop(s.conversations, outcome.conversation),
        notice: s.activeId === conversationId ? outcome.notice : s.notice,
      }));
    } catch (err) {
      if (get().activeId === conversationId) set({ error: message(err) });
    } finally {
      set({ streaming: null });
      await reloadMessages(conversationId).catch(() => {});
    }
  }

  return {
    ready: false,
    conversations: [],
    query: "",
    activeId: null,
    messages: [],
    loadingMessages: false,
    streaming: null,
    notice: null,
    error: null,
    keys: [],
    compatible: null,
    models: {},
    modelErrors: {},
    defaults: null,
    theme: "dark",
    settingsOpen: false,
    promptOpen: false,

    init: async () => {
      try {
        const [theme, provider, model] = await Promise.all([
          api.settings.get(SETTING_THEME),
          api.settings.get(SETTING_DEFAULT_PROVIDER),
          api.settings.get(SETTING_DEFAULT_MODEL),
        ]);
        const t: Theme = theme === "light" ? "light" : "dark";
        applyTheme(t);
        set({
          theme: t,
          defaults: provider && model ? { provider: provider as Provider, model } : null,
        });
        await Promise.all([get().refreshProviders(), get().loadConversations()]);
      } catch (err) {
        set({ error: message(err) });
      } finally {
        set({ ready: true });
      }
    },

    refreshProviders: async () => {
      const [keys, compatible] = await Promise.all([api.keys.statusAll(), api.compatible.get()]);
      set({ keys, compatible, models: {}, modelErrors: {} });
      const providers = keys.filter((k) => k.configured).map((k) => k.provider);
      await Promise.all(providers.map((p) => get().loadModels(p)));
      // Elegir el modelo por defecto cuando ya se conocen todas las listas.
      resolveDefaults();
    },

    loadModels: async (provider) => {
      try {
        const list = await api.models.list(provider);
        set((s) => ({
          models: { ...s.models, [provider]: list },
          modelErrors: { ...s.modelErrors, [provider]: undefined },
        }));
      } catch (err) {
        set((s) => ({ modelErrors: { ...s.modelErrors, [provider]: message(err) } }));
      }
    },

    setQuery: (query) => set({ query }),

    loadConversations: async () => {
      const q = get().query.trim();
      const conversations = q ? await api.conversations.search(q) : await api.conversations.list();
      set({ conversations });
    },

    select: async (id) => {
      if (get().activeId === id) return;
      set({ activeId: id, messages: [], notice: null, error: null, loadingMessages: true });
      try {
        await reloadMessages(id);
      } catch (err) {
        set({ error: message(err) });
      } finally {
        set({ loadingMessages: false });
      }
    },

    newChat: () => set({ activeId: null, messages: [], notice: null, error: null }),

    chooseModel: async (choice) => {
      const { activeId } = get();
      try {
        if (activeId) {
          const conv = await api.conversations.setModel(activeId, choice.provider, choice.model);
          set((s) => ({ conversations: s.conversations.map((c) => (c.id === conv.id ? conv : c)) }));
        }
        set({ defaults: choice });
        await Promise.all([
          api.settings.set(SETTING_DEFAULT_PROVIDER, choice.provider),
          api.settings.set(SETTING_DEFAULT_MODEL, choice.model),
        ]);
      } catch (err) {
        set({ error: message(err) });
      }
    },

    send: async (content, editFrom) => {
      const text = content.trim();
      if (!text || get().streaming) return;
      set({ error: null, notice: null });

      let conversationId = get().activeId;
      if (!conversationId) {
        const choice = get().defaults;
        if (!choice) {
          set({ error: "Elige un modelo (y configura su API key en Ajustes) antes de escribir." });
          return;
        }
        try {
          const conv = await api.conversations.create(choice.provider, choice.model);
          conversationId = conv.id;
          set((s) => ({ conversations: upsertTop(s.conversations, conv), activeId: conv.id, messages: [] }));
        } catch (err) {
          set({ error: message(err) });
          return;
        }
      }

      const id = conversationId;
      if (editFrom) {
        // Al editar, lo que venía después desaparece de inmediato (el backend lo borra igual).
        set((s) => {
          const index = s.messages.findIndex((m) => m.id === editFrom);
          return { messages: index >= 0 ? s.messages.slice(0, index) : s.messages };
        });
      }
      set({ streaming: { conversationId: id, text: "", pendingUser: text, kind: "send" } });

      await runStream(
        id,
        api.chat.send(
          id,
          text,
          (event) => {
            if (event.type === "started") {
              const saved = event.userMessage;
              set((s) => ({
                messages: saved && s.activeId === id ? [...s.messages, saved] : s.messages,
                streaming: s.streaming ? { ...s.streaming, pendingUser: null } : null,
              }));
            } else {
              set((s) =>
                s.streaming && s.streaming.conversationId === id
                  ? { streaming: { ...s.streaming, text: s.streaming.text + event.text } }
                  : {},
              );
            }
          },
          editFrom,
        ),
      );
    },

    regenerate: async () => {
      const { activeId, streaming, messages } = get();
      if (!activeId || streaming) return;
      set({ error: null, notice: null });
      const last = messages[messages.length - 1];
      if (last?.role === "assistant") set({ messages: messages.slice(0, -1) });
      set({ streaming: { conversationId: activeId, text: "", pendingUser: null, kind: "regenerate" } });
      await runStream(
        activeId,
        api.chat.regenerate(activeId, (event) => {
          if (event.type === "delta") {
            set((s) =>
              s.streaming ? { streaming: { ...s.streaming, text: s.streaming.text + event.text } } : {},
            );
          }
        }),
      );
    },

    stop: async () => {
      const s = get().streaming;
      if (s) await api.chat.stop(s.conversationId).catch(() => {});
    },

    rename: async (id, title) => {
      try {
        const conv = await api.conversations.rename(id, title);
        set((s) => ({ conversations: s.conversations.map((c) => (c.id === id ? conv : c)) }));
      } catch (err) {
        set({ error: message(err) });
      }
    },

    remove: async (id) => {
      try {
        await api.conversations.remove(id);
        set((s) => ({
          conversations: s.conversations.filter((c) => c.id !== id),
          ...(s.activeId === id ? { activeId: null, messages: [], notice: null } : {}),
        }));
      } catch (err) {
        set({ error: message(err) });
      }
    },

    setSystemPrompt: async (prompt) => {
      const id = get().activeId;
      if (!id) return;
      try {
        const conv = await api.conversations.setSystemPrompt(id, prompt);
        set((s) => ({ conversations: s.conversations.map((c) => (c.id === id ? conv : c)) }));
      } catch (err) {
        set({ error: message(err) });
      }
    },

    exportTo: async (path) => {
      const id = get().activeId;
      if (!id) return;
      try {
        await api.conversations.exportMarkdown(id, path);
        set({ notice: `Conversación exportada a ${path}` });
      } catch (err) {
        set({ error: message(err) });
      }
    },

    setTheme: async (theme) => {
      applyTheme(theme);
      set({ theme });
      await api.settings.set(SETTING_THEME, theme).catch(() => {});
    },

    setSettingsOpen: (open) => set({ settingsOpen: open }),
    setPromptOpen: (open) => set({ promptOpen: open }),
    dismissError: () => set({ error: null }),
  };
});

/** Proveedor y modelo que usará el próximo mensaje. */
export function useCurrentModel(): ModelChoice | null {
  // Se seleccionan valores simples: devolver un objeto nuevo en el selector
  // haría que React volviera a renderizar sin fin.
  const provider = useChat((s) => {
    const conv = s.activeId ? s.conversations.find((c) => c.id === s.activeId) : undefined;
    return conv?.provider ?? s.defaults?.provider ?? null;
  });
  const model = useChat((s) => {
    const conv = s.activeId ? s.conversations.find((c) => c.id === s.activeId) : undefined;
    return conv?.model ?? s.defaults?.model ?? null;
  });
  return useMemo(() => (provider && model ? { provider, model } : null), [provider, model]);
}
