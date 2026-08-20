import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ComfyUIDynamicField } from "./ComfyUIForm";
import type { WorkflowParam } from "@/types/comfyui";

function makeParam(overrides: Partial<WorkflowParam>): WorkflowParam {
  return {
    node_id: "8",
    widget_name: "seed",
    param_name: "8:seed",
    label: "seed",
    default_value: "0",
    field_type: "seed",
    order_index: 0,
    min: null,
    max: null,
    step: null,
    options: [],
    multiline: false,
    description: null,
    ...overrides,
  };
}

describe("ComfyUIDynamicField", () => {
  it("image_selector 在生图模式提示使用工作流默认图", () => {
    render(
      <ComfyUIDynamicField
        param={makeParam({ field_type: "image_selector" })}
        values={{}}
        setValues={() => {}}
        mode="generate"
      />,
    );
    expect(screen.getByText("生图时使用工作流默认图")).toBeInTheDocument();
  });

  it("image_selector 在编辑模式提示自动绑定所选原图", () => {
    render(
      <ComfyUIDynamicField
        param={makeParam({ field_type: "image_selector" })}
        values={{}}
        setValues={() => {}}
        mode="edit"
      />,
    );
    expect(screen.getByText("编辑时自动绑定所选原图")).toBeInTheDocument();
  });

  it("seed 随机按钮生成非负整数种子，而不是 -1", () => {
    let value = "0";
    render(
      <ComfyUIDynamicField
        param={makeParam({})}
        values={{ "8:seed": value }}
        setValues={(fn) => {
          value = fn({ "8:seed": value })["8:seed"];
        }}
        mode="generate"
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "🎲" }));
    const num = Number(value);
    expect(Number.isInteger(num)).toBe(true);
    expect(num).toBeGreaterThanOrEqual(0);
  });
});
