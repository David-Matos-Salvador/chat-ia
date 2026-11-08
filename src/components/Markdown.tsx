import { memo, useState, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeSanitize from "rehype-sanitize";
import rehypeHighlight from "rehype-highlight";
import { openUrl } from "@tauri-apps/plugin-opener";

/** Texto plano de un nodo hast (para copiar un bloque de código ya resaltado). */
interface HastNode {
  type: string;
  value?: string;
  tagName?: string;
  properties?: { className?: unknown };
  children?: HastNode[];
}

function hastText(node: HastNode | undefined): string {
  if (!node) return "";
  if (node.type === "text") return node.value ?? "";
  return (node.children ?? []).map(hastText).join("");
}

function languageOf(node: HastNode | undefined): string | null {
  const code = node?.children?.find((c) => c.tagName === "code");
  const classes = code?.properties?.className;
  const list = Array.isArray(classes) ? classes : [];
  const lang = list.map(String).find((c) => c.startsWith("language-"));
  return lang ? lang.slice("language-".length) : null;
}

export function CopyButton({
  text,
  label = "Copiar",
  onDark = false,
}: {
  text: string;
  label?: string;
  /** Sobre fondo siempre oscuro (bloques de código), en cualquier tema. */
  onDark?: boolean;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      onClick={() => {
        void navigator.clipboard.writeText(text).then(() => {
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        });
      }}
      className={`rounded px-2 py-0.5 font-mono text-xs ${
        onDark ? "text-slate-300 hover:bg-white/10 hover:text-cyan-300" : "text-muted hover:bg-panel-2 hover:text-accent"
      }`}
      aria-label={copied ? "Copiado" : label}
    >
      {copied ? "Copiado ✓" : label}
    </button>
  );
}

function CodeBlock({ node, children }: { node?: HastNode; children?: ReactNode }) {
  const lang = languageOf(node);
  return (
    <div className="overflow-hidden rounded-md border border-line bg-[#1e2230]">
      <div className="flex items-center justify-between border-b border-white/10 px-3 py-1">
        <span className="font-mono text-xs text-slate-300">{lang ?? "código"}</span>
        <CopyButton text={hastText(node).replace(/\n$/, "")} label="Copiar código" onDark />
      </div>
      <pre className="overflow-x-auto p-3 font-mono text-sm leading-relaxed text-slate-100">{children}</pre>
    </div>
  );
}

const components: Components = {
  pre: ({ node, children }) => <CodeBlock node={node as unknown as HastNode}>{children}</CodeBlock>,
  a: ({ href, children }) => (
    <a
      href={href}
      onClick={(e) => {
        // Los enlaces se abren en el navegador del sistema, no dentro de la app.
        e.preventDefault();
        if (href && /^https?:\/\//.test(href)) void openUrl(href);
      }}
    >
      {children}
    </a>
  ),
};

/**
 * Markdown de los mensajes. react-markdown no interpreta HTML crudo y además
 * se sanea el resultado; el resaltado se aplica después para conservar sus clases.
 */
export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeSanitize, [rehypeHighlight, { detect: true }]]}
        components={components}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
});
