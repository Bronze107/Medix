import { describe, it, expect } from "vitest";
import { workflowConsumesMask } from "./maskGuard";

/** loadImageMaskLink: LoadImage(1) 的 MASK 槽(1) 连到节点 2 */
const graph = (links: unknown[], nodeType = "LoadImage") =>
  JSON.stringify({
    nodes: [
      { id: 1, type: nodeType, outputs: [{ name: "IMAGE" }, { name: "MASK" }] },
      { id: 2, type: "SetLatentNoiseMask", inputs: [{ name: "mask" }] },
    ],
    links,
  });

describe("workflowConsumesMask", () => {
  it("LoadImage 的 MASK 输出有连线 → 消费蒙版", () => {
    expect(workflowConsumesMask(graph([[9, 1, 1, 2, 0, "MASK"]]))).toBe(true);
  });

  it("只用到 IMAGE 槽 → 不消费（这是最常见的误期望）", () => {
    expect(workflowConsumesMask(graph([[9, 1, 0, 2, 0, "IMAGE"]]))).toBe(false);
  });

  it("MASK 连线来自别的节点 → 不算（必须是 LoadImage 的输出）", () => {
    const json = JSON.stringify({
      nodes: [
        { id: 1, type: "LoadImage" },
        { id: 3, type: "MaskFromSomething" },
        { id: 2, type: "SetLatentNoiseMask" },
      ],
      links: [[9, 3, 1, 2, 0, "MASK"]],
    });
    expect(workflowConsumesMask(json)).toBe(false);
  });

  it("字符串节点 id 也要认（新版前端展平后会产生 \"459_451\" 这类 id）", () => {
    const json = JSON.stringify({
      nodes: [{ id: "459_451", type: "LoadImage" }, { id: 2, type: "X" }],
      links: [[9, "459_451", 1, 2, 0, "MASK"]],
    });
    expect(workflowConsumesMask(json)).toBe(true);
  });

  it("没有 LoadImage → 不消费", () => {
    expect(workflowConsumesMask(graph([[9, 1, 1, 2, 0, "MASK"]], "LoadImageMask"))).toBe(false);
  });

  it("畸形输入一律返回 false 而不是抛错", () => {
    expect(workflowConsumesMask(null)).toBe(false);
    expect(workflowConsumesMask(undefined)).toBe(false);
    expect(workflowConsumesMask("")).toBe(false);
    expect(workflowConsumesMask("not json")).toBe(false);
    expect(workflowConsumesMask("[1,2,3]")).toBe(false);
    expect(workflowConsumesMask("{}")).toBe(false);
    expect(workflowConsumesMask(JSON.stringify({ nodes: [], links: "nope" }))).toBe(false);
    // links 元素不是数组
    expect(workflowConsumesMask(graph([{ id: 9 }]))).toBe(false);
    expect(workflowConsumesMask(graph([[9, 1]]))).toBe(false);
  });
});
