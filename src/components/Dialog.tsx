import { useEffect, useId, useRef, type ReactNode } from "react";

/**
 * Ventana modal accesible basada en <dialog>: atrapa el foco, se cierra con Esc
 * y devuelve el foco al elemento anterior al cerrarse.
 */
export function Dialog({
  open,
  onClose,
  title,
  children,
  wide = false,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  wide?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      onClose={onClose}
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
      onClick={(e) => {
        // Clic en el fondo (fuera del contenido) cierra.
        if (e.target === ref.current) onClose();
      }}
      className={`m-auto max-h-[90vh] w-[calc(100%-2rem)] ${wide ? "max-w-3xl" : "max-w-lg"} overflow-hidden rounded-lg border border-line bg-panel p-0 text-text shadow-2xl`}
    >
      {open && (
        <div className="flex max-h-[90vh] flex-col">
          <div className="flex items-center justify-between border-b border-line px-5 py-3">
            <h2 id={titleId} className="font-display text-base tracking-wide text-text">
              {title}
            </h2>
            <button
              type="button"
              onClick={onClose}
              aria-label="Cerrar"
              className="rounded p-1 text-muted hover:bg-panel-2 hover:text-text"
            >
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
                <path d="M6 6l12 12M18 6L6 18" />
              </svg>
            </button>
          </div>
          <div className="overflow-y-auto px-5 py-4">{children}</div>
        </div>
      )}
    </dialog>
  );
}
