# Chat IA

App de escritorio para Windows tipo Claude / ChatGPT que usa tus propias API keys de Anthropic y OpenAI.

- **Interfaz:** React + TypeScript + Tailwind CSS 4 (Vite 6)
- **Backend:** Rust con Tauri 2
- **Datos:** historial en SQLite local; API keys en el Administrador de credenciales de Windows

## Requisitos

- Node.js 22.11 o superior
- Rust (stable, toolchain MSVC)
- Visual Studio Build Tools con la carga de trabajo "Desarrollo para el escritorio con C++"
- WebView2 (ya viene con Windows 11)

## Comandos

Instalar dependencias:

```bash
npm install
```

Abrir la app en modo desarrollo (recarga al guardar):

```bash
npm run tauri dev
```

Ejecutar las pruebas del backend:

```bash
cd src-tauri && cargo test
```

Crear el instalador de Windows (NSIS y MSI en `src-tauri/target/release/bundle/`):

```bash
npm run tauri build
```

## Estructura

```
chat-ia/
├── src/                  Interfaz React
│   ├── App.tsx           Pantalla principal
│   ├── index.css         Tema futurista (tokens de color y fuentes)
│   └── main.tsx          Punto de entrada
│   └── lib/api.ts        API tipada del backend (invoke)
└── src-tauri/            Backend Rust
    ├── src/lib.rs        Arranque: abre la base de datos y registra los comandos
    ├── src/commands.rs   Comandos que llama el frontend
    ├── src/error.rs      Error común ({ kind, message })
    ├── src/db/           SQLite: migraciones, modelos y consultas (con pruebas)
    ├── capabilities/     Permisos de la ventana
    └── tauri.conf.json   Ventana, seguridad (CSP) e instalador
```

## Estado

| Fase | Contenido | Estado |
|---|---|---|
| 0 | Entorno (Rust, Node, C++ Build Tools) | Hecho |
| 1 | Proyecto base, dependencias, tema y conexión React ↔ Rust | Hecho |
| 2 | Base de datos SQLite (esquema v1, migraciones, 13 pruebas) | Hecho |
| 3 | API keys en el Administrador de credenciales | Pendiente |
| 4 | Proveedores Claude y OpenAI con streaming | Pendiente |
| 5 | Comandos y canal de streaming | Pendiente |
| 6 | Interfaz completa del chat | Pendiente |
| 7 | Funciones extra | Pendiente |
| 8 | Empaquetado | Pendiente |
| 9 | Pruebas | Pendiente |

## Notas

- Vite está fijado en la versión 6 porque Vite 7+ necesita Node 22.12 o superior.
- Mantén el proyecto en una ruta corta (como `C:\dev\chat-ia`): Windows limita las rutas a 260 caracteres y una ruta larga rompe el enlazado de Rust (error LNK1104).
- `keyring` está fijado en la versión 3 por estabilidad de su API.
