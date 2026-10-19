import type { BrowseItem } from "@/types/browse";

/**
 * 把选中的 browse-item id 映射为去重后的 media id（变体共享 media_id），
 * 按浏览列表的显示顺序返回，剔除视频（AI 编辑仅支持图片）。
 * 全视频 / 空选择返回 []。
 */
export function pickAiEditMediaIds(
  selectedIds: Iterable<string>,
  items: BrowseItem[],
): string[] {
  const selected = selectedIds instanceof Set ? selectedIds : new Set(selectedIds);
  const out: string[] = [];
  for (const it of items) {
    if (!selected.has(it.id) || it.media_type === "video") continue;
    if (!out.includes(it.media_id)) out.push(it.media_id);
  }
  return out;
}
