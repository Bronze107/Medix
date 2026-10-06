/**
 * 判断工作流是否真的消费 `LoadImage` 的 MASK 输出。
 *
 * 为什么需要：蒙版是靠上传图的 **alpha 通道**送进去的（`LoadImage` 会把
 * MASK 算成 `1 - alpha`），但那只在**工作流把 LoadImage 的 MASK 输出接到
 * 某个节点**时才起作用。用户涂了半天下拉框却什么都没发生时，界面必须给出提示
 * —— 否则就是完全静默的失败。
 *
 * 判定是**启发式**的：只看标准画布格式的 `links`，子图内部连线、以及被
 * 打包成 API 格式的工作流都识别不到。所以调用方只应把它当作提示（警告横幅），
 * 不能拿它当硬性拦截。
 */

/** `LoadImage.RETURN_TYPES` 里 MASK 的槽位序号（IMAGE=0, MASK=1）。 */
const LOAD_IMAGE_MASK_SLOT = 1;

function asArray(v: unknown): unknown[] {
  return Array.isArray(v) ? v : [];
}

/**
 * 该工作流里是否存在「从某个 LoadImage 的 MASK 输出出发的连线」。
 * 返回 false 只代表**没检测到**，不代表一定不消费。
 */
export function workflowConsumesMask(workflowJson: string | null | undefined): boolean {
  if (!workflowJson) return false;
  let root: Record<string, unknown>;
  try {
    const parsed = JSON.parse(workflowJson);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return false;
    root = parsed as Record<string, unknown>;
  } catch {
    return false;
  }

  const nodes = asArray(root.nodes);
  if (nodes.length === 0) return false;

  // LoadImage 的节点 id（数字或字符串都要认，新版前端会产出字符串 id）
  const loadImageIds = new Set<string>();
  for (const node of nodes) {
    const n = node as Record<string, unknown>;
    if (n?.type === "LoadImage") {
      const id = n.id;
      if (typeof id === "string" || typeof id === "number") loadImageIds.add(String(id));
    }
  }
  if (loadImageIds.size === 0) return false;

  // 标准格式的连线是数组 [linkId, originId, originSlot, targetId, targetSlot, type]
  for (const link of asArray(root.links)) {
    if (!Array.isArray(link) || link.length < 3) continue;
    const [originId, originSlot] = [link[1], link[2]];
    if (originSlot !== LOAD_IMAGE_MASK_SLOT) continue;
    if (
      (typeof originId === "string" || typeof originId === "number") &&
      loadImageIds.has(String(originId))
    ) {
      return true;
    }
  }

  return false;
}
