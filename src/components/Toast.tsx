// Fluent-style toasts, top-right: intent icon, message and a close button.

import { CircleCheck, CircleX, Info, X } from 'lucide-react';
import { type ReactNode, createContext, useCallback, useContext, useMemo, useState } from 'react';

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

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const dismiss = useCallback((id: number) => setItems((prev) => prev.filter((t) => t.id !== id)), []);
  const show = useCallback(
    (kind: Kind, text: string) => {
      const id = Date.now() + Math.random();
      setItems((prev) => [...prev, { id, kind, text }]);
      window.setTimeout(() => dismiss(id), kind === 'error' ? 7000 : 3500);
    },
    [dismiss],
  );
  const api = useMemo(() => ({ show }), [show]);
  return (
    <ToastContext.Provider value={api}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        {items.map((t) => {
          const Icon = ICONS[t.kind];
          return (
            <div key={t.id} className={`toast toast-${t.kind}`}>
              <Icon size={20} className="toast-icon" aria-hidden="true" />
              <div className="toast-body">
                <div className="toast-title">{TITLES[t.kind]}</div>
                <div className="toast-text">{t.text}</div>
              </div>
              <button type="button" className="icon-btn icon-btn-sm" aria-label="Dismiss notification" onClick={() => dismiss(t.id)}>
                <X size={14} aria-hidden="true" />
              </button>
            </div>
          );
        })}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast(): ToastApi {
  return useContext(ToastContext);
}
