import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { mediaThumbnail, mediaThumbnailBatch } from "@/lib/tauri";

const MAX_CACHE_SIZE = 2000;
const cache = new Map<string, string>();

function cacheGet(key: string): string | undefined {
  const val = cache.get(key);
  if (val !== undefined) {
    // Move to end (most recently used)
    cache.delete(key);
    cache.set(key, val);
  }
  return val;
}

function cacheSet(key: string, value: string) {
  if (cache.has(key)) cache.delete(key);
  else while (cache.size >= MAX_CACHE_SIZE) {
    // Evict least recently used (first key in insertion order)
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
  cache.set(key, value);
}

/** Preload thumbnails for a batch of media IDs — one IPC call for all. */
export async function preloadThumbnails(ids: string[]): Promise<void> {
  const uncached = ids.filter((id) => !cache.has(id));
  if (uncached.length === 0) return;

  try {
    const results = await mediaThumbnailBatch(uncached);
    for (const r of results) {
      cacheSet(r.id, convertFileSrc(r.path));
    }
  } catch {
    // Silently fail — individual useThumbnail will retry
  }
}

export function useThumbnail(id: string | null, displayVariantId?: string | null) {
  const [url, setUrl] = useState<string | null>(null);
  const cacheKey = id ? (displayVariantId ? `${id}:${displayVariantId}` : id) : null;

  useEffect(() => {
    if (!id || !cacheKey) return;
    const cached = cacheGet(cacheKey);
    if (cached) {
      setUrl(cached);
      return;
    }

    let cancelled = false;
    const maxRetries = 3;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    const load = (attempt: number) => {
      mediaThumbnail(id)
        .then((path) => {
          if (!cancelled) {
            const src = convertFileSrc(path);
            cacheSet(cacheKey, src);
            setUrl(src);
          }
        })
        .catch(() => {
          if (!cancelled) {
            setUrl(null);
            if (attempt < maxRetries) {
              // Exponential backoff: 1s → 2s → 4s
              const delay = Math.min(1000 * Math.pow(2, attempt - 1), 4000);
              retryTimer = setTimeout(() => load(attempt + 1), delay);
            }
          }
        });
    };

    load(1);

    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
    };
  }, [id, cacheKey]);

  return url;
}
