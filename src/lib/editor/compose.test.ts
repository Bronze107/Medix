import { describe, it, expect } from "vitest";
import { planComposite, planOverlay } from "./compose";
import type { Stroke } from "./strokes";
import type { Size } from "./geometry";

const ORIG: Size = { w: 4000, h: 3000 };
// 工作分辨率恰好是原图的一半
const WORK: Size = { w: 2000, h: 1500 };

const stroke = (points: { x: number; y: number }[], size = 100): Stroke => ({
  id: "s1",
  points,
  size,
  hardness: 0.8,
  opacity: 1,
  color: "#ff0000",
  erase: false,
});

describe("planOverlay（实时预览，不裁剪）", () => {
  it("只按比例缩放，不做偏移", () => {
    const [s] = planOverlay([stroke([{ x: 400, y: 200 }])], ORIG, WORK);
    expect(s.points).toEqual([{ x: 200, y: 100 }]);
    // 笔尖直径是长度：只乘比例
    expect(s.size).toBe(50);
  });

  it("保留绘制属性", () => {
    const [s] = planOverlay([{ ...stroke([{ x: 0, y: 0 }]), erase: true, opacity: 0.5 }], ORIG, WORK);
    expect(s.erase).toBe(true);
    expect(s.opacity).toBe(0.5);
    expect(s.color).toBe("#ff0000");
  });
});

describe("planComposite（导出，含裁剪映射）", () => {
  it("无裁剪时输出尺寸 = 工作分辨率", () => {
    const plan = planComposite([], null, ORIG, WORK, false);
    expect(plan.sourceRect).toEqual({ x: 0, y: 0, w: 4000, h: 3000 });
    expect(plan.outputSize).toEqual({ w: 2000, h: 1500 });
  });

  it("裁剪后输出尺寸按裁剪区换算，源矩形即裁剪区", () => {
    const plan = planComposite([], { x: 1000, y: 600, w: 800, h: 600 }, ORIG, WORK, false);
    expect(plan.sourceRect).toEqual({ x: 1000, y: 600, w: 800, h: 600 });
    expect(plan.outputSize).toEqual({ w: 400, h: 300 });
  });

  it("笔迹先减裁剪原点再缩放（裁剪与涂抹顺序无关）", () => {
    const plan = planComposite(
      [stroke([{ x: 1000, y: 600 }, { x: 1400, y: 900 }])],
      { x: 1000, y: 600, w: 800, h: 600 },
      ORIG,
      WORK,
      false,
    );
    // 裁剪原点映射到 (0,0)，角点映射到输出尺寸
    expect(plan.strokes[0].points).toEqual([{ x: 0, y: 0 }, { x: 200, y: 150 }]);
    expect(plan.strokes[0].size).toBe(50);
  });

  it("裁剪区外的笔迹点仍被保留（交由 canvas clip 几何裁剪，不在这里丢点）", () => {
    const plan = planComposite(
      [stroke([{ x: 0, y: 0 }, { x: 4000, y: 3000 }])],
      { x: 1000, y: 600, w: 800, h: 600 },
      ORIG,
      WORK,
      false,
    );
    // 两个点都还在，只是坐标落在输出画布之外
    expect(plan.strokes[0].points).toHaveLength(2);
    expect(plan.strokes[0].points[0].x).toBeLessThan(0);
    expect(plan.strokes[0].points[1].x).toBeGreaterThan(plan.outputSize.w);
  });

  it("工作分辨率大于原图时不会反向缩小（scale 有实际比例）", () => {
    const plan = planComposite([], null, { w: 100, h: 100 }, { w: 100, h: 100 }, false);
    expect(plan.outputSize).toEqual({ w: 100, h: 100 });
  });

  it("退化输入不产生 0 尺寸输出", () => {
    const plan = planComposite([], { x: 0, y: 0, w: 0, h: 0 }, ORIG, WORK, false);
    expect(plan.outputSize.w).toBeGreaterThanOrEqual(1);
    expect(plan.outputSize.h).toBeGreaterThanOrEqual(1);
  });

  it("没有笔迹时输出空数组（不产生无谓的合成开销）", () => {
    expect(planComposite([], null, ORIG, WORK, false).strokes).toEqual([]);
  });
});

describe("planComposite 蒙版模式", () => {
  it("mask 标记透传到计划里（renderComposite 据此走 alpha 合成）", () => {
    expect(planComposite([], null, ORIG, WORK, true).mask).toBe(true);
    expect(planComposite([], null, ORIG, WORK, false).mask).toBe(false);
  });

  it("蒙版模式的坐标映射与画笔模式一致，只是最终合成方式不同", () => {
    const strokes = [stroke([{ x: 1000, y: 600 }])];
    const crop = { x: 1000, y: 600, w: 800, h: 600 };
    const asMask = planComposite(strokes, crop, ORIG, WORK, true);
    const asPaint = planComposite(strokes, crop, ORIG, WORK, false);
    expect(asMask.strokes).toEqual(asPaint.strokes);
    expect(asMask.outputSize).toEqual(asPaint.outputSize);
    expect(asMask.sourceRect).toEqual(asPaint.sourceRect);
  });
});
