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

// ---- 按档位（size）的缩略图缓存：`${id}:${tier}` -> asset url ----

const sizedCache = new Map<string, string>();
/** 每个媒体已请求过的最高档位（只升不降），防止缩放抖动反复拉取 */
const bestTierRequested = new Map<string, number>();

/** 同步读取已缓存的高档位缩略图。 */
export function cachedSizedThumbnail(id: string, tier: number): string | undefined {
  return sizedCache.get(`${id}:${tier}`);
}

function bestTierOf(id: string): number {
  return bestTierRequested.get(id) ?? 0;
}

/**
 * 批量获取高档位缩略图（一次 IPC 一档）。已缓存或已以更高档位请求过的
 * 条目跳过；后端懒生成失败的条目也会记入 bestTierRequested，避免无限重试。
 */
export async function fetchSizedThumbnails(
  reqs: { id: string; tier: number }[],
): Promise<void> {
  const byTier = new Map<number, string[]>();
  for (const { id, tier } of reqs) {
    if (tier <= bestTierOf(id)) continue;
    bestTierRequested.set(id, Math.max(bestTierOf(id), tier));
    if (sizedCache.has(`${id}:${tier}`)) continue;
    const list = byTier.get(tier) ?? [];
    list.push(id);
    byTier.set(tier, list);
  }
  if (byTier.size === 0) return;

  await Promise.all(
    [...byTier].map(async ([tier, ids]) => {
      try {
        const results = await mediaThumbnailBatch(ids, tier);
        for (const r of results) {
          sizedCache.set(`${r.id}:${tier}`, convertFileSrc(r.path));
        }
      } catch {
        // 静默失败 —— 前端继续显示基础档
      }
    }),
  );
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
