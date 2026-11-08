/**
 * Backend simulado para ver la interfaz en un navegador normal (`npm run dev`).
 * Solo se carga en desarrollo y fuera de Tauri; no forma parte de la app instalada.
 */
import { mockIPC } from "@tauri-apps/api/mocks";
import type { Channel } from "@tauri-apps/api/core";
import type { ChatOutcome, Conversation, Message, Provider, StreamEvent } from "../lib/api";

const now = Date.now();
const iso = (msAgo: number) => new Date(now - msAgo).toISOString();
const DAY = 86_400_000;

let seq = 0;
const id = () => `mock-${++seq}`;

const conversations: Conversation[] = [
  conv("Ideas para una app de recetas", "compatible", "abliterated-model", iso(5 * 60_000)),
  conv("Explicación de closures en JavaScript", "anthropic", "claude-opus-5-5", iso(DAY + 3_600_000)),
  conv("Plan de estudio de Rust", "compatible", "abliterated-model", iso(4 * DAY)),
  conv("Correo para el cliente", "anthropic", "claude-sonnet-5-5", iso(20 * DAY)),
];

function conv(title: string, provider: Provider, model: string, updatedAt: string): Conversation {
  return { id: id(), title, provider, model, systemPrompt: null, createdAt: updatedAt, updatedAt };
}

function msg(conversationId: string, role: Message["role"], content: string, tokens?: [number, number]): Message {
  return {
    id: id(),
    conversationId,
    role,
    content,
    model: role === "assistant" ? "abliterated-model" : null,
    inputTokens: tokens?.[0] ?? null,
    outputTokens: tokens?.[1] ?? null,
    createdAt: new Date().toISOString(),
  };
}

const messages: Record<string, Message[]> = {
  [conversations[0].id]: [
    msg(conversations[0].id, "user", "Dame 3 ideas para una app de recetas"),
    msg(
      conversations[0].id,
      "assistant",
      "Aquí tienes **tres ideas**:\n\n1. **Despensa inteligente**: sugiere recetas con lo que tienes.\n2. **Plan semanal**: genera menús y la lista de la compra.\n3. **Modo cocina**: pasos grandes y control por voz.\n\nUn ejemplo del modelo de datos:\n\n```ts\ninterface Receta {\n  titulo: string;\n  ingredientes: string[];\n  minutos: number;\n}\n```\n\n| Idea | Dificultad |\n|---|---|\n| Despensa | Media |\n| Plan semanal | Baja |",
      [19, 171],
    ),
  ],
};

const REPLY =
  "¡Claro! Esta es una **respuesta simulada** para probar la interfaz.\n\n- El texto llega por partes, como en el streaming real.\n- Puedes pulsar **Detener** en cualquier momento.\n\n```rust\nfn main() {\n    println!(\"Hola desde Chat IA\");\n}\n```";

const settings: Record<string, string> = {};
const stopped = new Set<string>();

async function streamReply(conversationId: string, channel: Channel<StreamEvent>): Promise<ChatOutcome> {
  stopped.delete(conversationId);
  let text = "";
  for (const part of REPLY.match(/.{1,6}/gs) ?? []) {
    if (stopped.has(conversationId)) break;
    await new Promise((r) => setTimeout(r, 25));
    text += part;
    channel.onmessage({ type: "delta", text: part });
  }
  const wasStopped = stopped.has(conversationId);
  const c = conversations.find((x) => x.id === conversationId)!;
  const assistant = msg(conversationId, "assistant", text, [42, 87]);
  messages[conversationId] = [...(messages[conversationId] ?? []), assistant];
  c.updatedAt = new Date().toISOString();
  return {
    conversation: c,
    userMessage: null,
    assistantMessage: assistant,
    stopReason: wasStopped ? null : "stop",
    notice: wasStopped ? "Respuesta detenida." : null,
    stopped: wasStopped,
  };
}

type Args = Record<string, any>;

export function installMockBackend() {
  mockIPC(async (cmd: string, args?: Args) => {
    const a = args ?? {};
    switch (cmd) {
      case "app_info":
        return { name: "Chat IA (simulado)", version: "0.1.0" };
      case "setting_get":
        return settings[a.key] ?? null;
      case "setting_set":
        settings[a.key] = a.value;
        return null;
      case "key_status_all":
        return [
          { provider: "anthropic", configured: true, masked: "sk-ant…a1b2" },
          { provider: "openai", configured: false, masked: null },
          { provider: "compatible", configured: true, masked: "ak_81V…nV1Q" },
        ];
      case "compatible_config_get":
        return { name: "Abliteration AI", baseUrl: "https://api.abliteration.ai/v1" };
      case "models_list":
        return a.provider === "anthropic"
          ? [
              { id: "claude-opus-5-5", name: "Claude Opus 5.5" },
              { id: "claude-sonnet-5-5", name: "Claude Sonnet 5.5" },
              { id: "claude-haiku-5-5", name: "Claude Haiku 5.5" },
            ]
          : [{ id: "abliterated-model", name: "Abliterated Model" }];
      case "conversation_list":
        return [...conversations].sort((x, y) => y.updatedAt.localeCompare(x.updatedAt));
      case "conversation_search": {
        const q = String(a.query).toLowerCase();
        return conversations.filter((c) => c.title.toLowerCase().includes(q));
      }
      case "conversation_create": {
        const c = conv("Nuevo chat", a.provider, a.model, new Date().toISOString());
        conversations.push(c);
        return c;
      }
      case "conversation_rename":
      case "conversation_set_model":
      case "conversation_set_system_prompt": {
        const c = conversations.find((x) => x.id === a.id)!;
        if (a.title) c.title = a.title;
        if (a.model) Object.assign(c, { provider: a.provider, model: a.model });
        if ("systemPrompt" in a) c.systemPrompt = a.systemPrompt;
        return c;
      }
      case "conversation_delete": {
        const i = conversations.findIndex((x) => x.id === a.id);
        if (i >= 0) conversations.splice(i, 1);
        return null;
      }
      case "message_list":
        return messages[a.conversationId] ?? [];
      case "chat_send": {
        const list = messages[a.conversationId] ?? [];
        if (a.editFrom) list.splice(list.findIndex((m) => m.id === a.editFrom));
        const user = msg(a.conversationId, "user", a.content);
        messages[a.conversationId] = [...list, user];
        const c = conversations.find((x) => x.id === a.conversationId)!;
        if (c.title === "Nuevo chat") c.title = String(a.content).slice(0, 48);
        a.onEvent.onmessage({ type: "started", userMessage: user });
        return { ...(await streamReply(a.conversationId, a.onEvent)), userMessage: user };
      }
      case "chat_regenerate": {
        const list = messages[a.conversationId] ?? [];
        if (list[list.length - 1]?.role === "assistant") list.pop();
        a.onEvent.onmessage({ type: "started", userMessage: null });
        return streamReply(a.conversationId, a.onEvent);
      }
      case "chat_stop":
        stopped.add(a.conversationId);
        return true;
      case "plugin:dialog|ask":
      case "plugin:dialog|confirm":
        return window.confirm(String(a.message ?? "¿Seguro?"));
      case "plugin:dialog|message":
        window.alert(String(a.message ?? ""));
        return null;
      case "plugin:dialog|save":
        return null;
      case "plugin:opener|open_url":
        window.open(String(a.url), "_blank");
        return null;
      default:
        console.warn("[mock] comando sin simular:", cmd, a);
        return null;
    }
  });
}
