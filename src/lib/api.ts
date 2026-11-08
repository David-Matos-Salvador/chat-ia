/**
 * API tipada del backend Rust.
 *
 * Cada función llama a un comando de Tauri con `invoke`. Los tipos reflejan
 * los structs de `src-tauri/src/db/models.rs`; si cambias uno, cambia el otro.
 */
import { invoke } from "@tauri-apps/api/core";

export type Role = "user" | "assistant" | "system";

export interface Conversation {
  id: string;
  title: string;
  model: string;
  systemPrompt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface Message {
  id: string;
  conversationId: string;
  role: Role;
  content: string;
  model: string | null;
  inputTokens: number | null;
  outputTokens: number | null;
  createdAt: string;
}

export interface AppInfo {
  name: string;
  version: string;
}

export interface DbStatus {
  schemaVersion: number;
  conversations: number;
  messages: number;
  path: string;
}

export type ApiErrorKind = "db" | "not_found" | "invalid" | "io" | "internal" | "unknown";

/** Error que devuelve el backend (`AppError` en Rust). */
export class ApiError extends Error {
  constructor(
    public readonly kind: ApiErrorKind,
    message: string,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

function toApiError(err: unknown): ApiError {
  if (err && typeof err === "object" && "kind" in err && "message" in err) {
    const e = err as { kind: ApiErrorKind; message: string };
    return new ApiError(e.kind, e.message);
  }
  return new ApiError("unknown", err instanceof Error ? err.message : String(err));
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (err) {
    throw toApiError(err);
  }
}

export const api = {
  appInfo: () => call<AppInfo>("app_info"),
  dbStatus: () => call<DbStatus>("db_status"),

  conversations: {
    create: (model: string, title?: string, systemPrompt?: string) =>
      call<Conversation>("conversation_create", { model, title, systemPrompt }),
    list: () => call<Conversation[]>("conversation_list"),
    get: (id: string) => call<Conversation>("conversation_get", { id }),
    search: (query: string) => call<Conversation[]>("conversation_search", { query }),
    rename: (id: string, title: string) => call<Conversation>("conversation_rename", { id, title }),
    setModel: (id: string, model: string) => call<Conversation>("conversation_set_model", { id, model }),
    setSystemPrompt: (id: string, systemPrompt: string | null) =>
      call<Conversation>("conversation_set_system_prompt", { id, systemPrompt }),
    remove: (id: string) => call<void>("conversation_delete", { id }),
  },

  messages: {
    list: (conversationId: string) => call<Message[]>("message_list", { conversationId }),
  },

  settings: {
    get: (key: string) => call<string | null>("setting_get", { key }),
    set: (key: string, value: string) => call<void>("setting_set", { key, value }),
  },
};
