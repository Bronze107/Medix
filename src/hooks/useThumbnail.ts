import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { mediaThumbnail } from "@/lib/tauri";

const cache = new Map<string, string>();

export function useThumbnail(id: string | null, displayVariantId?: string | null) {
  const [url, setUrl] = useState<string | null>(null);
  const cacheKey = id ? (displayVariantId ? `${id}:${displayVariantId}` : id) : null;

  useEffect(() => {
    if (!id || !cacheKey) return;
    if (cache.has(cacheKey)) {
      setUrl(cache.get(cacheKey)!);
      return;
    }

    let cancelled = false;
    let retryCount = 0;
    const maxRetries = 15;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    const load = () => {
      mediaThumbnail(id)
        .then((path) => {
          if (!cancelled) {
            const src = convertFileSrc(path);
            cache.set(cacheKey, src);
            setUrl(src);
          }
        })
        .catch(() => {
          if (!cancelled) {
            setUrl(null);
            retryCount++;
            if (retryCount <= maxRetries) {
              retryTimer = setTimeout(load, 2000);
            }
          }
        });
    };

    load();

    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
    };
  }, [id, cacheKey]);

  return url;
}
