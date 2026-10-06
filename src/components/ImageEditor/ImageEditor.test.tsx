import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ImageEditor } from "./ImageEditor";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => p }));
vi.mock("@/lib/tauri", () => ({
  mediaGetPaths: vi.fn(async () => ({ original: "asset://p.png", thumb_256: null })),
}));

/**
 * jsdom 里 `getContext("2d")` 返回 null、`getBoundingClientRect` 全为 0，
 * 所以这里打两个桩，让组件的**非像素**行为（工具切换、撤销栈、确认产物）
 * 能被真实驱动。像素结果本身由 strokes/compose 的纯函数单测覆盖。
 */
const ctxStub = {
  clearRect: () => {},
  fillRect: () => {},
  drawImage: () => {},
  save: () => {},
  restore: () => {},
  beginPath: () => {},
  rect: () => {},
  clip: () => {},
  arc: () => {},
  fill: () => {},
  createRadialGradient: () => ({ addColorStop: () => {} }),
  globalAlpha: 1,
  globalCompositeOperation: "source-over",
  fillStyle: "",
};

beforeEach(() => {
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(
    ctxStub as unknown as CanvasRenderingContext2D,
  );
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/png;base64,AAAA");
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    width: 400,
    height: 300,
    left: 0,
    top: 0,
    right: 400,
    bottom: 300,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  } as DOMRect);
});

/** 渲染并触发图片加载（jsdom 不会自己设 naturalWidth）。 */
async function mountEditor() {
  const onConfirm = vi.fn();
  const onCancel = vi.fn();
  const utils = render(<ImageEditor mediaId="m1" onConfirm={onConfirm} onCancel={onCancel} />);
  // 底图 alt="" 是装饰性的（role=presentation），不能用 findByRole("img")
  const img = await vi.waitFor(() => {
    const el = utils.container.querySelector("img");
    if (!el || !el.getAttribute("src")) throw new Error("图片尚未挂载");
    return el as HTMLImageElement;
  });
  Object.defineProperty(img, "naturalWidth", { value: 400, configurable: true });
  Object.defineProperty(img, "naturalHeight", { value: 300, configurable: true });
  fireEvent.load(img);
  return { ...utils, onConfirm, onCancel };
}

const toolTab = (name: string) => screen.getByRole("button", { name });
const paintCanvas = () => screen.getByTestId("paint-canvas");

describe("ImageEditor", () => {
  it("默认裁剪模式显示比例按钮，切到画笔显示笔刷参数", async () => {
    await mountEditor();
    expect(toolTab("1:1")).toBeTruthy();
    expect(screen.queryByLabelText("画笔大小")).toBeNull();

    fireEvent.click(toolTab("画笔"));
    expect(screen.getByLabelText("画笔大小")).toBeTruthy();
    expect(screen.getByLabelText("画笔硬度")).toBeTruthy();
    expect(toolTab("橡皮擦")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "1:1" })).toBeNull();
  });

  it("无改动时：撤销/重做/清空/保存都禁用，取消不弹确认框", async () => {
    const { onCancel } = await mountEditor();
    // 撤销/重做/清空只属于画笔工具 —— 裁剪改动本就不进撤销栈，在裁剪模式下
    // 显示这些按钮会误导
    fireEvent.click(toolTab("画笔"));
    expect(toolTab("撤销")).toBeDisabled();
    expect(toolTab("重做")).toBeDisabled();
    expect(toolTab("清空")).toBeDisabled();
    expect(toolTab("保存新版本")).toBeDisabled();

    fireEvent.click(toolTab("取消"));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("放弃编辑？")).toBeNull();
  });

  it("画一笔后启用撤销与清空；撤销后启用重做", async () => {
    await mountEditor();
    fireEvent.click(toolTab("画笔"));

    const canvas = paintCanvas();
    fireEvent.pointerDown(canvas, { clientX: 10, clientY: 10 });
    fireEvent.pointerUp(canvas, { clientX: 10, clientY: 10 });

    expect(toolTab("撤销")).toBeEnabled();
    expect(toolTab("清空")).toBeEnabled();
    expect(toolTab("重做")).toBeDisabled();
    expect(toolTab("保存新版本")).toBeEnabled();

    fireEvent.click(toolTab("撤销"));
    expect(toolTab("撤销")).toBeDisabled();
    expect(toolTab("重做")).toBeEnabled();
    expect(toolTab("保存新版本")).toBeDisabled();

    fireEvent.click(toolTab("重做"));
    expect(toolTab("撤销")).toBeEnabled();
  });

  it("有改动时点取消会弹确认框", async () => {
    await mountEditor();
    fireEvent.click(toolTab("画笔"));
    const canvas = paintCanvas();
    fireEvent.pointerDown(canvas, { clientX: 10, clientY: 10 });
    fireEvent.pointerUp(canvas, { clientX: 10, clientY: 10 });

    fireEvent.click(toolTab("取消"));
    expect(screen.getByText("放弃编辑？")).toBeTruthy();
  });

  it("有笔迹时保存走 canvas 合成（否则笔迹会被丢掉）", async () => {
    const { onConfirm } = await mountEditor();
    fireEvent.click(toolTab("画笔"));
    const canvas = paintCanvas();
    fireEvent.pointerDown(canvas, { clientX: 10, clientY: 10 });
    fireEvent.pointerUp(canvas, { clientX: 10, clientY: 10 });

    fireEvent.click(toolTab("保存新版本"));
    await vi.waitFor(() => expect(onConfirm).toHaveBeenCalledTimes(1));
    expect(onConfirm.mock.calls[0][0]).toEqual({
      kind: "canvas",
      dataUrl: "data:image/png;base64,AAAA",
      // library 模式下有笔迹 = 有可见笔迹，不是蒙版
      hasMask: false,
    });
  });

  it("comfy-edit 模式：工具是裁剪 + 蒙版，产物恒为 canvas 并标记 hasMask", async () => {
    const onConfirm = vi.fn();
    const utils = render(
      <ImageEditor mediaId="m1" mode="comfy-edit" onConfirm={onConfirm} onCancel={vi.fn()} />,
    );
    const img = await vi.waitFor(() => {
      const el = utils.container.querySelector("img");
      if (!el || !el.getAttribute("src")) throw new Error("图片尚未挂载");
      return el as HTMLImageElement;
    });
    Object.defineProperty(img, "naturalWidth", { value: 400, configurable: true });
    Object.defineProperty(img, "naturalHeight", { value: 300, configurable: true });
    fireEvent.load(img);

    // 工具页签是「蒙版」而不是「画笔」
    expect(toolTab("蒙版")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "画笔" })).toBeNull();

    fireEvent.click(toolTab("蒙版"));
    const canvas = paintCanvas();
    fireEvent.pointerDown(canvas, { clientX: 10, clientY: 10 });
    fireEvent.pointerUp(canvas, { clientX: 10, clientY: 10 });

    fireEvent.click(toolTab("保存新版本"));
    await vi.waitFor(() => expect(onConfirm).toHaveBeenCalledTimes(1));
    expect(onConfirm.mock.calls[0][0]).toMatchObject({ kind: "canvas", hasMask: true });
  });

  it("comfy-edit 模式：只改裁剪也必须走 canvas（蒙版要随图送出）", async () => {
    const onConfirm = vi.fn();
    const utils = render(
      <ImageEditor mediaId="m1" mode="comfy-edit" onConfirm={onConfirm} onCancel={vi.fn()} />,
    );
    const img = await vi.waitFor(() => {
      const el = utils.container.querySelector("img");
      if (!el || !el.getAttribute("src")) throw new Error("图片尚未挂载");
      return el as HTMLImageElement;
    });
    Object.defineProperty(img, "naturalWidth", { value: 400, configurable: true });
    Object.defineProperty(img, "naturalHeight", { value: 300, configurable: true });
    fireEvent.load(img);

    fireEvent.click(toolTab("1:1"));
    fireEvent.click(toolTab("保存新版本"));
    await vi.waitFor(() => expect(onConfirm).toHaveBeenCalledTimes(1));
    // 走的是 canvas 而不是无损 crop 路径
    expect(onConfirm.mock.calls[0][0]).toMatchObject({ kind: "canvas", hasMask: false });
  });

  it("只有裁剪改动时走无损裁剪路径（不经画布）", async () => {
    const { onConfirm } = await mountEditor();
    // 拖右上角手柄把裁剪框改小 —— 这里直接用比例预设更稳定
    fireEvent.click(toolTab("1:1"));
    fireEvent.click(toolTab("保存新版本"));
    await vi.waitFor(() => expect(onConfirm).toHaveBeenCalledTimes(1));
    const arg = onConfirm.mock.calls[0][0];
    expect(arg.kind).toBe("crop");
    expect(arg.rect.w).toBe(arg.rect.h); // 1:1
  });
});
