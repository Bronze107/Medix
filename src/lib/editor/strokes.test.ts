import { describe, it, expect } from "vitest";
import { STAMP_SPACING_RATIO, stampCenters, strokeIntersectsRect, tipRadius, type Stroke } from "./strokes";

const stroke = (points: { x: number; y: number }[], size = 20): Stroke => ({
  id: "s1",
  points,
  size,
  hardness: 0.8,
  opacity: 1,
  color: "#000",
  erase: false,
});

describe("stampCenters", () => {
  it("空笔迹不产生落点", () => {
    expect(stampCenters([], 20)).toEqual([]);
  });

  it("单点（点击一下）原样返回一个落点", () => {
    expect(stampCenters([{ x: 5, y: 7 }], 20)).toEqual([{ x: 5, y: 7 }]);
  });

  it("沿直线按间距铺点，含首尾", () => {
    // size 20 → spacing 5；长度 100 → 首点 + 20 个
    const pts = stampCenters([{ x: 0, y: 0 }, { x: 100, y: 0 }], 20);
    expect(pts.length).toBe(21);
    expect(pts[0]).toEqual({ x: 0, y: 0 });
    expect(pts[20].x).toBeCloseTo(100, 6);
    expect(pts[1].x).toBeCloseTo(5, 6);
  });

  it("间距跨段连续：不会在折点处空一段", () => {
    // 两段各长 100，总长 200，spacing 5 → 首点 + 40 个
    const pts = stampCenters(
      [
        { x: 0, y: 0 },
        { x: 100, y: 0 },
        { x: 100, y: 100 },
      ],
      20,
    );
    expect(pts.length).toBe(41);
    // 折点右侧第一个点应落在第二段上，且距折点 5
    const kinkIdx = pts.findIndex((p) => p.y > 0);
    expect(pts[kinkIdx].y).toBeCloseTo(5, 6);
  });

  it("长度不是间距整数倍时，余量带到下一段", () => {
    // 第一段 103（余 3），第二段从距折点 2 处开始铺
    const pts = stampCenters(
      [
        { x: 0, y: 0 },
        { x: 103, y: 0 },
        { x: 103, y: 50 },
      ],
      20,
    );
    const onSecond = pts.filter((p) => p.y > 0);
    expect(onSecond[0].y).toBeCloseTo(2, 6);
  });

  it("零长度段被跳过而不是死循环", () => {
    const pts = stampCenters(
      [
        { x: 10, y: 10 },
        { x: 10, y: 10 },
        { x: 30, y: 10 },
      ],
      20,
    );
    expect(pts.length).toBeGreaterThan(1);
    expect(pts.every((p) => Number.isFinite(p.x) && Number.isFinite(p.y))).toBe(true);
  });

  it("退化笔尖尺寸不产生无限循环", () => {
    const pts = stampCenters([{ x: 0, y: 0 }, { x: 100, y: 0 }], 0);
    expect(pts).toEqual([{ x: 0, y: 0 }, { x: 100, y: 0 }]);
  });

  it("间距比例符合常量", () => {
    expect(STAMP_SPACING_RATIO).toBeLessThan(0.5);
    const pts = stampCenters([{ x: 0, y: 0 }, { x: 100, y: 0 }], 10);
    // spacing = 2.5 → 首点 + 40 个
    expect(pts.length).toBe(41);
  });
});

describe("tipRadius", () => {
  it("至少为 0.5，避免半径为 0 的渐变", () => {
    expect(tipRadius(0)).toBe(0.5);
    expect(tipRadius(20)).toBe(10);
  });
});

describe("strokeIntersectsRect", () => {
  const rect = { x: 100, y: 100, w: 50, h: 50 };

  it("笔迹落在框外（含笔尖半径余量）判为不相交", () => {
    expect(strokeIntersectsRect(stroke([{ x: 0, y: 0 }]), rect)).toBe(false);
    expect(strokeIntersectsRect(stroke([{ x: 500, y: 500 }]), rect)).toBe(false);
  });

  it("笔迹落在框内或压到边框判为相交", () => {
    expect(strokeIntersectsRect(stroke([{ x: 120, y: 120 }]), rect)).toBe(true);
    // 笔尖半径 10，圆心在框外 5px 处仍会压到边框
    expect(strokeIntersectsRect(stroke([{ x: 95, y: 120 }], 20), rect)).toBe(true);
  });

  it("空心笔迹（无点）不相交", () => {
    expect(strokeIntersectsRect(stroke([]), rect)).toBe(false);
  });
});
