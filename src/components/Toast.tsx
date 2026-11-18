// Fluent-style toasts, top-right: intent icon, message and a close button. Errors stay until dismissed and are
// announced at once; confirmations go after a few seconds, never while hovered or focused.

import { CircleCheck, CircleX, Info, X } from 'lucide-react';
import { type ReactNode, createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';

type Kind = 'success' | 'error' | 'info';
interface ToastItem {
  id: number;
  kind: Kind;
  text: string;
}

interface ToastApi {
  show: (kind: Kind, text: string) => void;
}

const ToastContext = createContext<ToastApi>({ show: () => undefined });

const ICONS = { success: CircleCheck, error: CircleX, info: Info };
const TITLES = { success: 'Done', error: 'Something went wrong', info: 'Note' };

/** How long a success or note stays (paused while hovered or focused); errors stay until dismissed. */
const SHOW_MS = 5000;
let nextId = 1;

function ToastView({ toast, onDismiss }: { toast: ToastItem; onDismiss: (id: number) => void }) {
  const [paused, setPaused] = useState(false);
  useEffect(() => {
    if (toast.kind === 'error' || paused) return undefined;
    const timer = window.setTimeout(() => onDismiss(toast.id), SHOW_MS);
    return () => window.clearTimeout(timer);
  }, [toast, paused, onDismiss]);
  const Icon = ICONS[toast.kind];
  return (
    <div
      className={`toast toast-${toast.kind}`}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
    >
      <Icon size={20} className="toast-icon" aria-hidden="true" />
      <div className="toast-body">
        <div className="toast-title">{TITLES[toast.kind]}</div>
        <div className="toast-text">{toast.text}</div>
      </div>
      <button type="button" className="icon-btn icon-btn-sm" aria-label="Dismiss notification" onClick={() => onDismiss(toast.id)}>
        <X size={14} aria-hidden="true" />
      </button>
    </div>
  );
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const dismiss = useCallback((id: number) => setItems((prev) => prev.filter((t) => t.id !== id)), []);
  const show = useCallback((kind: Kind, text: string) => {
    const id = nextId++;
    setItems((prev) => [...prev, { id, kind, text }]);
  }, []);
  const api = useMemo(() => ({ show }), [show]);
  const errors = items.filter((t) => t.kind === 'error');
  const others = items.filter((t) => t.kind !== 'error');
  return (
    <ToastContext.Provider value={api}>
      {children}
      <div className="toasts">
        {/* Errors are announced at once (assertive); confirmations wait their turn (polite). */}
        <div aria-live="assertive" className="toast-region">
          {errors.map((t) => (
            <ToastView key={t.id} toast={t} onDismiss={dismiss} />
          ))}
        </div>
        <div role="status" aria-live="polite" className="toast-region">
          {others.map((t) => (
            <ToastView key={t.id} toast={t} onDismiss={dismiss} />
          ))}
        </div>
      </div>
    </ToastContext.Provider>
  );
}

export function useToast(): ToastApi {
  return useContext(ToastContext);
}
