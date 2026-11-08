# Chat IA

App de escritorio para Windows tipo Claude / ChatGPT que usa tus propias API keys de **Anthropic (Claude)**, **OpenAI** o cualquier servicio **compatible con la API de OpenAI** (Abliteration AI, OpenRouter, Ollama…).

- **Interfaz:** React + TypeScript + Tailwind CSS 4 (Vite 6), tema oscuro futurista o claro
- **Backend:** Rust con Tauri 2
- **Datos:** historial en SQLite local; API keys cifradas en el Administrador de credenciales de Windows

## Funciones

- Chat con respuestas en **streaming** (token a token) y botón **Detener**
- Historial con **búsqueda**, agrupado por fecha; renombrar y borrar conversaciones
- **Selector de modelo** con la lista real de cada proveedor
- Mensajes en **Markdown** con código resaltado, tablas y botón de copiar
- **Regenerar** la última respuesta y **editar** mensajes anteriores
- **Instrucciones del sistema** por conversación
- **Exportar** una conversación a Markdown
- **Uso de tokens** por mensaje y total de la conversación
- Atajos de teclado y accesibilidad (navegación con teclado, foco visible, lectores de pantalla)

## Uso

1. Abre **Ajustes** (Ctrl + ,) y añade tu API key en la fila del proveedor:
   - **Anthropic (Claude)**: keys `sk-ant-…`
   - **OpenAI**: keys `sk-…`
   - **Compatible con OpenAI**: pulsa un atajo (por ejemplo **Abliteration AI**) o escribe la URL base, y pega tu key (`ak_…` en Abliteration)
2. Elige el modelo arriba a la derecha.
3. Escribe y pulsa **Enter**.

| Atajo | Acción |
|---|---|
| Enter / Shift + Enter | Enviar / salto de línea |
| Esc | Detener la respuesta |
| Ctrl + N | Nuevo chat |
| Ctrl + K | Buscar conversaciones |
| Ctrl + , | Ajustes |
| Ctrl + + / Ctrl + - (o Ctrl + rueda) | Agrandar / reducir el texto (80 %–150 %, se recuerda) |
| Ctrl + 0 | Tamaño de texto normal |

## Requisitos para desarrollar

- Node.js 22.11 o superior
- Rust (stable, toolchain MSVC)
- Visual Studio Build Tools con "Desarrollo para el escritorio con C++"
- WebView2 (ya viene con Windows 11)

## Comandos

```bash
npm install
```

App en modo desarrollo (recarga al guardar):

```bash
npm run tauri dev
```

Solo la interfaz en el navegador, con un backend simulado (útil para diseñar):

```bash
npm run dev
```

Pruebas del backend (Rust) y de la interfaz:

```bash
cd src-tauri && cargo test
```

```bash
npm test
```

Prueba contra el Administrador de credenciales real (crea y borra una key de prueba):

```bash
cd src-tauri && cargo test -- --ignored
```

Instaladores de Windows (quedan en `src-tauri/target/release/bundle/nsis` y `.../msi`):

```bash
npm run tauri build
```

## Estructura

```
chat-ia/
├── src/                        Interfaz React
│   ├── App.tsx                 Disposición general y atajos
│   ├── store/chat.ts           Estado (Zustand): conversaciones, streaming, modelos
│   ├── components/             Sidebar, ChatHeader, MessageList, Composer, Markdown,
│   │                           SettingsDialog, KeysPanel, SystemPromptDialog…
│   ├── lib/api.ts              API tipada del backend (invoke + Channel)
│   ├── lib/format.ts           Utilidades de presentación (con pruebas)
│   └── dev/mockBackend.ts      Backend simulado para el navegador (no entra en el build)
└── src-tauri/                  Backend Rust
    ├── src/lib.rs              Arranque y registro de comandos
    ├── src/commands.rs         Comandos que llama la interfaz
    ├── src/chat.rs             Envío, streaming, detener y guardado de respuestas
    ├── src/db/                 SQLite: migraciones (v2), modelos y consultas
    ├── src/keys.rs             API keys en el Administrador de credenciales
    ├── src/providers/          Anthropic, OpenAI y compatibles: comprobación de keys,
    │                           lista de modelos, lector SSE y streaming
    └── tauri.conf.json         Ventana, seguridad (CSP) e instaladores
```

## Cómo funciona el streaming

1. La interfaz llama a `chat_send` con un `Channel`.
2. Rust guarda tu mensaje, lee la key del Administrador de credenciales y envía el historial al proveedor con `stream: true`.
3. Cada trozo de texto llega por SSE; el lector trabaja por bytes, así que no rompe caracteres partidos entre trozos.
4. Rust reenvía cada trozo a la interfaz por el `Channel` y, al terminar (o al detener), guarda la respuesta con su uso de tokens.

Detalles por proveedor:
- **Claude**: `POST /v1/messages`, versión `2026-10-21`, `max_tokens` 64 000 y `fallbacks: "default"` en los modelos que lo admiten (si el modelo rechaza una petición, Anthropic la reintenta en otro).
- **OpenAI y compatibles**: `POST {base}/chat/completions`. El razonamiento del modelo (`thinking`, `delta.reasoning`) no se muestra; solo la respuesta.

## Lista de comprobación manual

- [ ] Guardar y probar una key en cada proveedor que uses
- [ ] Enviar un mensaje y ver la respuesta llegar por partes
- [ ] Detener una respuesta larga: lo recibido queda guardado
- [ ] Regenerar la última respuesta
- [ ] Editar un mensaje anterior: lo posterior se reemplaza
- [ ] Poner instrucciones en un chat y comprobar que se respetan
- [ ] Buscar, renombrar y borrar conversaciones
- [ ] Exportar una conversación a Markdown
- [ ] Cambiar entre tema oscuro y claro
- [ ] Cerrar y abrir la app: el historial y el tema se conservan

## Estado

| Fase | Contenido | Estado |
|---|---|---|
| 0 | Entorno (Rust, Node, C++ Build Tools) | Hecho |
| 1 | Proyecto base, tema y conexión React ↔ Rust | Hecho |
| 2 | Base de datos SQLite | Hecho |
| 3 | API keys en el Administrador de credenciales y proveedor compatible | Hecho |
| 4 | Streaming con Claude, OpenAI y compatibles | Hecho |
| 5 | Comandos de chat: enviar, detener, regenerar, editar | Hecho |
| 6 | Interfaz completa | Hecho |
| 7 | Instrucciones, exportar, tokens | Hecho |
| 8 | Icono e instaladores NSIS y MSI | Hecho |
| 9 | Pruebas (Rust y Vitest) y documentación | Hecho |

Pendiente para una versión futura: adjuntar imágenes (visión) y varios proveedores compatibles a la vez.

## Notas

- Vite está fijado en la versión 6 porque Vite 7+ necesita Node 22.12 o superior.
- Mantén el proyecto en una ruta corta (como `C:\dev\chat-ia`): Windows limita las rutas a 260 caracteres y una ruta larga rompe el enlazado de Rust (error LNK1104).
- `keyring` está fijado en la versión 3 por estabilidad de su API.
- Los instaladores no están firmados: Windows SmartScreen mostrará un aviso al instalar ("Más información" → "Ejecutar de todas formas").
