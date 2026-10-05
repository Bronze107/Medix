/**
 * 编辑器文档与撤销栈。
 *
 * **撤销采用「命令栈重放」而不是 `ImageData` 快照**（`docs/IMAGE_TOOLS.md §5`
 * 建议的是快照）：2048² 的一张 ImageData 就是 16MB，20 步 ≈ 320MB，而且快照
 * 只存在于 canvas 里、无法单测。重放的内存开销是 O(笔迹数)，且是纯函数，可以
 * 直接测。代价是撤销后要重绘整层笔迹 —— 笔迹本身很轻，可接受。
 *
 * 撤销范围**只含笔画**：裁剪有自己的可见控件（比例按钮 + 拖拽），把它也塞进
 * 撤销栈会让「Ctrl+Z 到底撤销了画笔还是裁剪」变得不可预测。
 */

import type { Stroke } from "./strokes";

/** 撤销栈上限。笔画数据很小，可以比快照方案给得宽裕。 */
export const HISTORY_LIMIT = 50;

export interface HistoryState {
  strokes: Stroke[];
  past: Stroke[][];
  future: Stroke[][];
}

export type HistoryAction =
  | { type: "addStroke"; stroke: Stroke }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "clear" };

export function initialHistory(): HistoryState {
  return { strokes: [], past: [], future: [] };
}

export function canUndo(state: HistoryState): boolean {
  return state.past.length > 0;
}

export function canRedo(state: HistoryState): boolean {
  return state.future.length > 0;
}

export function historyReducer(state: HistoryState, action: HistoryAction): HistoryState {
  switch (action.type) {
    case "addStroke": {
      const past = [...state.past, state.strokes];
      // 超出上限丢最旧的一步
      if (past.length > HISTORY_LIMIT) past.shift();
      return { strokes: [...state.strokes, action.stroke], past, future: [] };
    }
    case "undo": {
      if (state.past.length === 0) return state;
      const past = [...state.past];
      const previous = past.pop() as Stroke[];
      return { strokes: previous, past, future: [state.strokes, ...state.future] };
    }
    case "redo": {
      if (state.future.length === 0) return state;
      const [next, ...future] = state.future;
      return { strokes: next, past: [...state.past, state.strokes], future };
    }
    case "clear": {
      if (state.strokes.length === 0) return state;
      const past = [...state.past, state.strokes];
      if (past.length > HISTORY_LIMIT) past.shift();
      return { strokes: [], past, future: [] };
    }
    default:
      return state;
  }
}

/** 是否有任何可保存的改动。 */
export function hasEdits(state: HistoryState, cropTouched: boolean): boolean {
  return state.strokes.length > 0 || cropTouched;
}
