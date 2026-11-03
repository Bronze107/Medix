/**
 * 缩略图档位选择。与 Rust 端 `media::thumbnail::pick_tier` 保持同一规则：
 *
 * - 256 是基础档，永远存在（生成时钳制到原图内，不放大）
 * - 512 / 1024 高档位仅当不超过原图最长边时才会生成
 * - 渲染需求 ≤ 256px 直接用基础档；否则在可用高档位中选最小的满足档，
 *   没有则退到最大的可用高档位；原图太小没有高档位时回到基础档
 */
export const BASE_TIER = 256;
export const HIGH_TIERS = [512, 1024] as const;

export type ThumbTier = 256 | 512 | 1024;

/**
 * @param neededPx 渲染所需像素（CSS 尺寸 × devicePixelRatio，最长边）
 * @param originalLongestEdge 原图最长边；未知或非法时按没有高档位处理
 */
export function pickThumbnailTier(
  neededPx: number,
  originalLongestEdge: number | null | undefined,
): ThumbTier {
  const need = Math.max(1, Math.ceil(Number.isFinite(neededPx) ? neededPx : 0));
  if (need <= BASE_TIER) return BASE_TIER;

  const longest = originalLongestEdge != null && originalLongestEdge > 0 ? originalLongestEdge : 0;
  const candidates = HIGH_TIERS.filter((t) => t <= longest);
  if (candidates.length === 0) return BASE_TIER;

  for (const t of candidates) {
    if (t >= need) return t;
  }
  return candidates[candidates.length - 1];
}
