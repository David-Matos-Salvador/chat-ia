import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/orbitron/600.css";
import "@fontsource-variable/inter";
import "@fontsource/jetbrains-mono/400.css";
import "./index.css";
import App from "./App";

function render() {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

// En desarrollo, fuera de Tauri (un navegador normal), usar el backend simulado
// para poder ver y probar la interfaz. En la app real este bloque no se ejecuta.
const inTauri = "__TAURI_INTERNALS__" in window;
if (import.meta.env.DEV && !inTauri) {
  import("./dev/mockBackend").then((m) => {
    m.installMockBackend();
    render();
  });
} else {
  render();
}
