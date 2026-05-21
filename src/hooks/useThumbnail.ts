import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { mediaThumbnail } from "@/lib/tauri";

const cache = new Map<string, string>();

export function useThumbnail(id: string | null) {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    if (!id) return;
    if (cache.has(id)) {
      setUrl(cache.get(id)!);
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
            cache.set(id, src);
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
  }, [id]);

  return url;
}
