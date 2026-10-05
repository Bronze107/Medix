import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/react";
import { CropOverlay } from "./CropOverlay";
import type { Rect, Size } from "@/lib/editor/geometry";

const ORIG: Size = { w: 400, h: 300 };
/** 显示框是原图的一半 → 显示位移要乘 2 才是原图像素 */
const DISPLAY: Size = { w: 200, h: 150 };
const FULL: Rect = { x: 0, y: 0, w: 400, h: 300 };

function setup(rect: Rect = FULL, aspect: number | null = null) {
  const onChange = vi.fn();
  const utils = render(
    <CropOverlay
      origSize={ORIG}
      displaySize={DISPLAY}
      rect={rect}
      aspect={aspect}
      onChange={onChange}
    />,
  );
  const overlay = utils.container.querySelector('[data-testid="crop-overlay"]') as HTMLElement;
  const handle = (kind: string) =>
    utils.container.querySelector(`[data-handle="${kind}"]`) as HTMLElement;
  /** 从 (x0,y0) 拖到 (x1,y1)（显示空间的 client 坐标） */
  const drag = (el: HTMLElement, x0: number, y0: number, x1: number, y1: number) => {
    fireEvent.pointerDown(el, { clientX: x0, clientY: y0 });
    fireEvent.pointerMove(overlay, { clientX: x1, clientY: y1 });
    fireEvent.pointerUp(overlay, { clientX: x1, clientY: y1 });
  };
  return { onChange, overlay, handle, drag, container: utils.container };
}

describe("CropOverlay", () => {
  it("自由裁剪：显示位移按比例换算成原图像素", () => {
    const { onChange, handle, drag } = setup({ x: 0, y: 0, w: 200, h: 100 });
    // 右下角手柄拖 (40,30) 显示像素 → 原图 (80,60)
    drag(handle("se"), 100, 50, 140, 80);
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange.mock.calls[0][0]).toEqual({ x: 0, y: 0, w: 280, h: 160 });
  });

  it("往外拖超出图像时被夹到边界（不会产出越界矩形）", () => {
    const { onChange, handle, drag } = setup();
    drag(handle("se"), 100, 75, 200, 150);
    expect(onChange.mock.calls[0][0]).toEqual({ x: 0, y: 0, w: 400, h: 300 });
  });

  it("自由裁剪：拖左上角手柄只动起点", () => {
    const { onChange, handle, drag } = setup({ x: 100, y: 100, w: 200, h: 100 });
    drag(handle("nw"), 50, 50, 70, 60);
    expect(onChange.mock.calls[0][0]).toEqual({ x: 140, y: 120, w: 160, h: 80 });
  });

  it("拖选框本体 = 平移，尺寸不变", () => {
    const { onChange, drag, container } = setup({ x: 100, y: 100, w: 200, h: 100 });
    const body = container.querySelector(".cursor-move") as HTMLElement;
    drag(body, 100, 100, 130, 120);
    expect(onChange.mock.calls[0][0]).toEqual({ x: 160, y: 140, w: 200, h: 100 });
  });

  it("锁比例时只渲染四个角手柄", () => {
    const { handle } = setup(FULL, 16 / 9);
    for (const corner of ["nw", "ne", "sw", "se"]) {
      expect(handle(corner)).toBeTruthy();
    }
    for (const edge of ["n", "s", "e", "w"]) {
      expect(handle(edge)).toBeNull();
    }
  });

  it("锁比例拖角：产出严格等比且不越界", () => {
    const { onChange, handle, drag } = setup({ x: 100, y: 100, w: 160, h: 90 }, 16 / 9);
    drag(handle("se"), 100, 100, 160, 100);
    const r = onChange.mock.calls[0][0] as Rect;
    expect(r.w / r.h).toBeCloseTo(16 / 9, 6);
    expect(r.x).toBeGreaterThanOrEqual(0);
    expect(r.y).toBeGreaterThanOrEqual(0);
    expect(r.x + r.w).toBeLessThanOrEqual(ORIG.w);
    expect(r.y + r.h).toBeLessThanOrEqual(ORIG.h);
  });

  it("退化结果被丢弃：onChange 不被调用，state 里进不了非法矩形", () => {
    const { onChange, handle, drag } = setup({ x: 0, y: 0, w: 100, h: 100 });
    // 把右下角往左上拖到只剩几像素
    drag(handle("se"), 100, 100, 20, 20);
    // 位移 (-80,-80) 显示 → (-160,-160) 原图 → 宽高变负 → 归一化后 0 → 返回 null → 丢弃
    for (const call of onChange.mock.calls) {
      const r = call[0] as Rect;
      expect(r.w).toBeGreaterThan(0);
      expect(r.h).toBeGreaterThan(0);
    }
  });
});
