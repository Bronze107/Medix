import { useCallback, useState } from "react";

const MAX_HISTORY = 10;
const STORAGE_KEY_GENERATE = "prompt_history_generate";
const STORAGE_KEY_EDIT = "prompt_history_edit";

export interface PromptEntry {
  prompt: string;
  aspectRatio: string;
  resolution: string;
  time: number; // Date.now()
}

function load(key: string): PromptEntry[] {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return [];
    const parsed: PromptEntry[] = JSON.parse(raw);
    return parsed.slice(0, MAX_HISTORY);
  } catch {
    return [];
  }
}

function save(key: string, entries: PromptEntry[]) {
  try {
    localStorage.setItem(key, JSON.stringify(entries));
  } catch { /* quota exceeded, ignore */ }
}

export function usePromptHistory(type: "generate" | "edit") {
  const storageKey = type === "edit" ? STORAGE_KEY_EDIT : STORAGE_KEY_GENERATE;
  const [items, setItems] = useState<PromptEntry[]>(() => load(storageKey));

  const record = useCallback(
    (prompt: string, aspectRatio: string, resolution: string) => {
      const trimmed = prompt.trim();
      if (!trimmed) return;
      setItems((prev) => {
        const filtered = prev.filter((p) => p.prompt !== trimmed);
        const next = [{ prompt: trimmed, aspectRatio, resolution, time: Date.now() }, ...filtered].slice(0, MAX_HISTORY);
        save(storageKey, next);
        return next;
      });
    },
    [storageKey],
  );

  const clear = useCallback(() => {
    setItems([]);
    save(storageKey, []);
  }, [storageKey]);

  return { items, record, clear };
}
