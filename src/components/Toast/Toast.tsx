import { useCallback, useEffect, useRef, useState } from "react";

interface ToastItem {
  id: number;
  message: string;
  type: "info" | "error";
}

let nextId = 0;
const listeners = new Set<() => void>();
const toasts: ToastItem[] = [];

export function showToast(message: string, type: "info" | "error" = "info") {
  toasts.push({ id: nextId++, message, type });
  listeners.forEach((fn) => fn());
}

const DURATION = 2000;
const EXIT_ANIM = 300;

function Toast() {
  const [items, setItems] = useState<ToastItem[]>([]);
  const [exiting, setExiting] = useState<Set<number>>(new Set());
  const timers = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  const remove = useCallback((id: number) => {
    setExiting((prev) => new Set(prev).add(id));
    setTimeout(() => {
      const idx = toasts.findIndex((t) => t.id === id);
      if (idx !== -1) toasts.splice(idx, 1);
      setItems([...toasts]);
      setExiting((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
      timers.current.delete(id);
    }, EXIT_ANIM);
  }, []);

  useEffect(() => {
    const listener = () => {
      const now = [...toasts];
      setItems(now);
      for (const t of now) {
        if (!timers.current.has(t.id)) {
          timers.current.set(
            t.id,
            setTimeout(() => remove(t.id), DURATION),
          );
        }
      }
    };
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
      timers.current.forEach((t) => clearTimeout(t));
    };
  }, [remove]);

  if (items.length === 0) return null;

  return (
    <div className="pointer-events-none fixed bottom-6 right-6 z-[100] flex flex-col gap-2">
      {items.map((t) => (
        <div
          key={t.id}
          className={`rounded-lg px-4 py-2.5 text-xs shadow-lg backdrop-blur-xl transition-all duration-300 ${
            exiting.has(t.id) ? "opacity-0 translate-y-2" : "animate-fade-in-up"
          } ${
            t.type === "error"
              ? "bg-[var(--color-danger-soft)]/90 text-[var(--color-danger)]"
              : "bg-[var(--color-bg-elevated)]/90 text-[var(--color-text-primary)]"
          }`}
        >
          {t.message}
        </div>
      ))}
    </div>
  );
}

export default Toast;
