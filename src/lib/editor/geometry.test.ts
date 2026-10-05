import { describe, it, expect } from "vitest";
import {
  MIN_CROP_PX,
  WORK_MAX_DIM,
  clampFreeRect,
  computeWorkingSize,
  displayFromOriginal,
  fitAspectRect,
  fullRect,
  applyDrag,
  lockAspectFromAnchor,
  originalFromWorking,
  serializeCropRect,
  workingFromClient,
  type HandleKind,
  type Rect,
  type Size,
} from "./geometry";

const ORIG: Size = { w: 4000, h: 3000 };

describe("computeWorkingSize", () => {
  it("超过最长边上限时等比缩小", () => {
    const work = computeWorkingSize(4000, 3000);
    expect(Math.max(work.w, work.h)).toBe(WORK_MAX_DIM);
    // 比例保持（允许 1px 取整误差）
    expect(work.w / work.h).toBeCloseTo(4000 / 3000, 2);
  });

  it("小图不放大", () => {
    expect(computeWorkingSize(800, 600)).toEqual({ w: 800, h: 600 });
  });

  it("极端长条也至少 1px，不出现 0", () => {
    const work = computeWorkingSize(100000, 3);
    expect(work.w).toBe(WORK_MAX_DIM);
    expect(work.h).toBeGreaterThanOrEqual(1);
  });

  it("退化输入不产生 0 或 NaN", () => {
    expect(computeWorkingSize(0, 0)).toEqual({ w: 1, h: 1 });
  });
});

describe("坐标映射", () => {
  const work: Size = { w: 2000, h: 1000 };

  it("workingFromClient 用实时 rect 换算，而不是缓存的显示尺寸", () => {
    // 显示 1000x500，工作 2000x1000 → 2 倍
    const p = workingFromClient(110, 60, { left: 10, top: 10, width: 1000, height: 500 }, work);
    expect(p).toEqual({ x: 200, y: 100 });
  });

  it("workingFromClient 对零尺寸 rect 返回原点而不是 Infinity/NaN", () => {
    const p = workingFromClient(50, 50, { left: 0, top: 0, width: 0, height: 0 }, work);
    expect(p).toEqual({ x: 0, y: 0 });
  });

  it("originalFromWorking 与 displayFromOriginal 互相自洽", () => {
    // 工作 2 倍于显示、原图 2 倍于工作
    const o = originalFromWorking(100, 50, work, ORIG);
    expect(o.x).toBeCloseTo(200, 6);
    expect(o.y).toBeCloseTo(150, 6);

    // 原图 → 显示（假设显示框 = 工作框的 1/4）
    const d = displayFromOriginal(2000, 1500, ORIG, { w: 500, h: 375 });
    expect(d).toEqual({ x: 250, y: 187.5 });
  });
});

describe("serializeCropRect", () => {
  it("起点 floor、终点 ceil，宁可多含一行", () => {
    // x: floor(10.7)=10 → ceil(10.7+100.1)=ceil(110.8)=111 → w=101
    // y: floor(20.2)=20 → ceil(20.2+50.9)=ceil(71.1)=72  → h=52
    const r: Rect = { x: 10.7, y: 20.2, w: 100.1, h: 50.9 };
    expect(serializeCropRect(r, ORIG)).toEqual({ x: 10, y: 20, w: 101, h: 52 });
  });

  it("越界部分夹回图像内", () => {
    const r: Rect = { x: -50, y: -50, w: 500, h: 500 };
    expect(serializeCropRect(r, ORIG)).toEqual({ x: 0, y: 0, w: 450, h: 450 });

    const over: Rect = { x: 3900, y: 2900, w: 500, h: 500 };
    expect(serializeCropRect(over, ORIG)).toEqual({ x: 3900, y: 2900, w: 100, h: 100 });
  });

  it("整图裁剪原样返回", () => {
    expect(serializeCropRect(fullRect(ORIG), ORIG)).toEqual({ x: 0, y: 0, w: 4000, h: 3000 });
  });

  it("小于最小尺寸时返回 null（确认按钮据此禁用）", () => {
    expect(serializeCropRect({ x: 10, y: 10, w: 2, h: 2 }, ORIG)).toBeNull();
    expect(serializeCropRect({ x: 10, y: 10, w: 0, h: 50 }, ORIG)).toBeNull();
    expect(serializeCropRect({ x: 10, y: 10, w: MIN_CROP_PX - 1, h: 50 }, ORIG)).toBeNull();
    expect(serializeCropRect({ x: 10, y: 10, w: MIN_CROP_PX, h: MIN_CROP_PX }, ORIG)).not.toBeNull();
  });
});

describe("clampFreeRect（自由裁剪，两轴独立）", () => {
  it("夹进边界并保留尺寸", () => {
    expect(clampFreeRect({ x: -20, y: -20, w: 300, h: 200 }, ORIG)).toEqual({
      x: 0,
      y: 0,
      w: 300,
      h: 200,
    });
    expect(clampFreeRect({ x: 3950, y: 2950, w: 300, h: 200 }, ORIG)).toEqual({
      x: 3700,
      y: 2800,
      w: 300,
      h: 200,
    });
  });

  it("比图像还大时缩到图像尺寸", () => {
    expect(clampFreeRect({ x: 0, y: 0, w: 9999, h: 9999 }, ORIG)).toEqual({
      x: 0,
      y: 0,
      w: 4000,
      h: 3000,
    });
  });

  it("过小返回 null", () => {
    expect(clampFreeRect({ x: 0, y: 0, w: 3, h: 3 }, ORIG)).toBeNull();
  });
});

describe("fitAspectRect（等比裁剪）", () => {
  it("超出边界时整体缩放，比例不漂移", () => {
    // 1:1 的框放在 4:3 图上，且尺寸超过图高
    const r = fitAspectRect({ x: 0, y: 0, w: 5000, h: 5000 }, ORIG, 1);
    expect(r).not.toBeNull();
    // 缩放后贴住短边（高 3000），并且严格 1:1
    expect(r!.h).toBeCloseTo(3000, 6);
    expect(r!.w).toBeCloseTo(3000, 6);
    expect(r!.w / r!.h).toBeCloseTo(1, 6);
  });

  it("只平移不缩放，比例保持", () => {
    const r = fitAspectRect({ x: 2000, y: 1500, w: 1000, h: 500 }, ORIG, 2);
    expect(r).toEqual({ x: 2000, y: 1500, w: 1000, h: 500 });
  });

  it("负起点被推回边界内且比例不变", () => {
    const r = fitAspectRect({ x: -300, y: -300, w: 1600, h: 900 }, ORIG, 16 / 9);
    expect(r!.x).toBe(0);
    expect(r!.y).toBe(0);
    expect(r!.w / r!.h).toBeCloseTo(16 / 9, 6);
  });

  it("缩放后小于最小尺寸返回 null", () => {
    expect(fitAspectRect({ x: 0, y: 0, w: 4, h: 4 }, ORIG, 1)).toBeNull();
    expect(fitAspectRect({ x: 0, y: 0, w: 100, h: 100 }, ORIG, 0)).toBeNull();
  });
});

describe("applyDrag", () => {
  const r: Rect = { x: 100, y: 100, w: 200, h: 100 };

  it("move 只平移", () => {
    expect(applyDrag(r, "move", 30, -20, null)).toEqual({ x: 130, y: 80, w: 200, h: 100 });
  });

  it("自由缩放：角与边分别只影响对应边界", () => {
    expect(applyDrag(r, "se", 10, 20, null)).toEqual({ x: 100, y: 100, w: 210, h: 120 });
    expect(applyDrag(r, "nw", 10, 20, null)).toEqual({ x: 110, y: 120, w: 190, h: 80 });
    expect(applyDrag(r, "e", 10, 999, null)).toEqual({ x: 100, y: 100, w: 210, h: 100 });
    expect(applyDrag(r, "n", 999, 20, null)).toEqual({ x: 100, y: 120, w: 200, h: 80 });
  });

  it("自由缩放拖过对边时归一化成正的宽高", () => {
    const out = applyDrag(r, "se", -300, -200, null);
    expect(out.w).toBeGreaterThan(0);
    expect(out.h).toBeGreaterThan(0);
    expect(out).toEqual({ x: 0, y: 0, w: 100, h: 100 });
  });

  it("锁比例时四角各自锚定对角，比例严格", () => {
    const corners: [HandleKind, { x: number; y: number }][] = [
      ["se", { x: 100, y: 100 }],
      ["sw", { x: 300, y: 100 }],
      ["ne", { x: 100, y: 200 }],
      ["nw", { x: 300, y: 200 }],
    ];
    for (const [kind, anchor] of corners) {
      const out = applyDrag(r, kind, 60, 10, 16 / 9);
      expect(out.w / out.h).toBeCloseTo(16 / 9, 6);
      const corners4 = [
        { x: out.x, y: out.y },
        { x: out.x + out.w, y: out.y },
        { x: out.x, y: out.y + out.h },
        { x: out.x + out.w, y: out.y + out.h },
      ];
      const hit = corners4.some(
        (c) => Math.abs(c.x - anchor.x) < 1e-6 && Math.abs(c.y - anchor.y) < 1e-6,
      );
      expect(hit, `${kind} 应固定住对角 ${JSON.stringify(anchor)}`).toBe(true);
    }
  });

  it("锁比例时边手柄退化为自由缩放（UI 在锁比例时不显示边手柄）", () => {
    const out = applyDrag(r, "e", 10, 0, 1);
    expect(out).toEqual({ x: 100, y: 100, w: 210, h: 100 });
  });
});

describe("lockAspectFromAnchor", () => {
  const anchor = { x: 1000, y: 1000 };

  it("四个方向都产出严格等比矩形，且锚点不动", () => {
    const dirs = [
      { x: 1600, y: 1200 },
      { x: 400, y: 1200 },
      { x: 400, y: 800 },
      { x: 1600, y: 800 },
    ];
    for (const moving of dirs) {
      const r = lockAspectFromAnchor(anchor, moving, 4 / 3);
      expect(r.w / r.h).toBeCloseTo(4 / 3, 6);
      // 锚点必须落在矩形的一角
      const corners = [
        { x: r.x, y: r.y },
        { x: r.x + r.w, y: r.y },
        { x: r.x, y: r.y + r.h },
        { x: r.x + r.w, y: r.y + r.h },
      ];
      const hit = corners.some(
        (c) => Math.abs(c.x - anchor.x) < 1e-6 && Math.abs(c.y - anchor.y) < 1e-6,
      );
      expect(hit).toBe(true);
    }
  });

  it("方向随落点所在象限翻转", () => {
    const r = lockAspectFromAnchor(anchor, { x: 400, y: 800 }, 1);
    expect(r.x + r.w).toBeCloseTo(1000, 6);
    expect(r.y + r.h).toBeCloseTo(1000, 6);
  });

  it("零高度拖动不产生 NaN/Infinity", () => {
    const r = lockAspectFromAnchor(anchor, { x: 1400, y: 1000 }, 1);
    expect(Number.isFinite(r.w)).toBe(true);
    expect(Number.isFinite(r.h)).toBe(true);
  });
});
