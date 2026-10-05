import { describe, it, expect } from "vitest";
import {
  HISTORY_LIMIT,
  canRedo,
  canUndo,
  hasEdits,
  historyReducer,
  initialHistory,
  type HistoryAction,
  type HistoryState,
} from "./document";
import type { Stroke } from "./strokes";

const stroke = (id: string): Stroke => ({
  id,
  points: [{ x: 0, y: 0 }],
  size: 20,
  hardness: 1,
  opacity: 1,
  color: "#000",
  erase: false,
});

function run(actions: HistoryAction[], start = initialHistory()): HistoryState {
  return actions.reduce(historyReducer, start);
}

describe("historyReducer", () => {
  it("初始状态不可撤销/重做", () => {
    const s = initialHistory();
    expect(s.strokes).toEqual([]);
    expect(canUndo(s)).toBe(false);
    expect(canRedo(s)).toBe(false);
  });

  it("加笔画后可撤销，撤销后可重做", () => {
    const s = run([{ type: "addStroke", stroke: stroke("a") }]);
    expect(s.strokes.map((x) => x.id)).toEqual(["a"]);
    expect(canUndo(s)).toBe(true);
    expect(canRedo(s)).toBe(false);

    const undone = historyReducer(s, { type: "undo" });
    expect(undone.strokes).toEqual([]);
    expect(canUndo(undone)).toBe(false);
    expect(canRedo(undone)).toBe(true);

    const redone = historyReducer(undone, { type: "redo" });
    expect(redone.strokes.map((x) => x.id)).toEqual(["a"]);
  });

  it("多步撤销按逆序回退，且保持顺序", () => {
    let s = run([
      { type: "addStroke", stroke: stroke("a") },
      { type: "addStroke", stroke: stroke("b") },
      { type: "addStroke", stroke: stroke("c") },
    ]);
    s = historyReducer(s, { type: "undo" });
    expect(s.strokes.map((x) => x.id)).toEqual(["a", "b"]);
    s = historyReducer(s, { type: "undo" });
    expect(s.strokes.map((x) => x.id)).toEqual(["a"]);
    s = historyReducer(s, { type: "redo" });
    expect(s.strokes.map((x) => x.id)).toEqual(["a", "b"]);
  });

  it("新操作清空重做栈", () => {
    let s = run([{ type: "addStroke", stroke: stroke("a") }]);
    s = historyReducer(s, { type: "undo" });
    expect(canRedo(s)).toBe(true);
    s = historyReducer(s, { type: "addStroke", stroke: stroke("b") });
    expect(canRedo(s)).toBe(false);
    expect(s.strokes.map((x) => x.id)).toEqual(["b"]);
  });

  it("空栈上的 undo/redo 是空操作（不抛错、不破坏状态）", () => {
    const s = initialHistory();
    expect(historyReducer(s, { type: "undo" })).toBe(s);
    expect(historyReducer(s, { type: "redo" })).toBe(s);
    const withStroke = run([{ type: "addStroke", stroke: stroke("a") }]);
    expect(historyReducer(withStroke, { type: "redo" })).toBe(withStroke);
  });

  it("清空本身可撤销（清空后还能撤回来）", () => {
    let s = run([
      { type: "addStroke", stroke: stroke("a") },
      { type: "addStroke", stroke: stroke("b") },
      { type: "clear" },
    ]);
    expect(s.strokes).toEqual([]);
    s = historyReducer(s, { type: "undo" });
    expect(s.strokes.map((x) => x.id)).toEqual(["a", "b"]);
  });

  it("空文档上的 clear 不产生历史记录", () => {
    const s = initialHistory();
    expect(historyReducer(s, { type: "clear" })).toBe(s);
  });

  it("超出上限时丢最旧的一步，内存有界", () => {
    const actions: HistoryAction[] = [];
    for (let i = 0; i < HISTORY_LIMIT + 10; i++) {
      actions.push({ type: "addStroke", stroke: stroke(`s${i}`) });
    }
    const s = run(actions);
    expect(s.past.length).toBe(HISTORY_LIMIT);
    // 仍保留最新的笔画
    expect(s.strokes[s.strokes.length - 1].id).toBe(`s${HISTORY_LIMIT + 9}`);
  });
});

describe("hasEdits", () => {
  it("无笔画且未改裁剪时无改动", () => {
    expect(hasEdits(initialHistory(), false)).toBe(false);
  });

  it("有笔画或改过裁剪都算有改动", () => {
    expect(hasEdits(initialHistory(), true)).toBe(true);
    expect(hasEdits(run([{ type: "addStroke", stroke: stroke("a") }]), false)).toBe(true);
  });
});
