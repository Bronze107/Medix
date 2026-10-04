import type { WorkflowParam } from "@/types/comfyui";

/**
 * 按工作流记忆参数表单的「上一次的值」。
 *
 * 存在 localStorage（单键 map，key 为 workflow id）：这些是本机 UI 偏好，
 * 不需要进数据库；`settings` 表的 `settings_get_all` 只枚举固定白名单键，
 * 没有按工作流 id 动态存取的能力。
 *
 * 只在提交成功后写入，因此「上一次」严格等于上一次真正跑过的参数。
 */
const STORAGE_KEY = "medix.comfyuiWorkflowValues";

type MemoryMap = Record<string, Record<string, string>>;

function load(): MemoryMap {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    return parsed as MemoryMap;
  } catch {
    // 存储不可用或内容损坏：当作没有记忆
    return {};
  }
}

function save(map: MemoryMap) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(map));
  } catch {
    // 配额超限等，忽略
  }
}

/** 工作流自身的默认值，image_selector 用其默认值占位（不参与记忆）。 */
export function defaultWorkflowValues(params: WorkflowParam[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const p of params) {
    out[p.param_name] = p.default_value;
  }
  return out;
}

/**
 * 加载某个工作流的初始表单值：以工作流默认值为底，叠加记忆的上次值。
 * 只叠加仍存在于 `params` 中的键 —— 工作流被改过、参数已删除时自动丢弃陈旧值。
 * image_selector 永不采用记忆值（编辑模式运行时绑定，生图模式用工作流默认图）。
 */
export function mergeWorkflowValues(
  workflowId: string,
  params: WorkflowParam[],
): Record<string, string> {
  const merged = defaultWorkflowValues(params);
  const remembered = load()[workflowId];
  if (!remembered) return merged;
  for (const p of params) {
    if (p.field_type === "image_selector") continue;
    const v = remembered[p.param_name];
    if (typeof v === "string") merged[p.param_name] = v;
  }
  return merged;
}

/** 记住本次提交的参数（提交成功后调用）。 */
export function rememberWorkflowValues(
  workflowId: string,
  params: WorkflowParam[],
  values: Record<string, string>,
): void {
  const toStore: Record<string, string> = {};
  for (const p of params) {
    if (p.field_type === "image_selector") continue;
    const v = values[p.param_name];
    if (typeof v === "string") toStore[p.param_name] = v;
  }
  const map = load();
  map[workflowId] = toStore;
  save(map);
}

/** 清除某个工作流的记忆值（恢复工作流默认值）。 */
export function clearWorkflowValues(workflowId: string): void {
  const map = load();
  if (!(workflowId in map)) return;
  delete map[workflowId];
  save(map);
}
