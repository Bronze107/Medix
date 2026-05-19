import { useEffect, useState } from "react";

interface ToastItem {
  id: number;
  message: string;
  type: "info" | "error";
}

let nextId = 0;
const listeners = new Set<() => void>();
let toasts: ToastItem[] = [];

export function showToast(message: string, type: "info" | "error" = "info") {
  toasts = [...toasts, { id: nextId++, message, type }];
  listeners.forEach((fn) => fn());
}

function Toast() {
  const [items, setItems] = useState<ToastItem[]>([]);

  useEffect(() => {
    const listener = () => setItems([...toasts]);
    listeners.add(listener);
    return () => { listeners.delete(listener); };
  }, []);

  useEffect(() => {
    if (items.length === 0) return;
    const timer = setTimeout(() => {
      toasts = toasts.filter((t) => !items.some((i) => i.id === t.id));
      setItems([]);
    }, 2000);
    return () => clearTimeout(timer);
  }, [items]);

  if (items.length === 0) return null;

  return (
    <div className="fixed bottom-4 right-4 z-[100] flex flex-col gap-1.5">
      {items.map((t) => (
        <div
          key={t.id}
          className={`rounded-lg px-4 py-2 text-xs shadow-lg ${
            t.type === "error"
              ? "bg-red-900/90 text-red-200"
              : "bg-gray-800/90 text-gray-200"
          }`}
        >
          {t.message}
        </div>
      ))}
    </div>
  );
}

export default Toast;
