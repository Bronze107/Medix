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

  function renderSlider(initial: string) {
    let values: Record<string, string> = { "8:seed": initial };
    render(
      <ComfyUIDynamicField
        param={makeParam({ field_type: "slider", label: "strength", min: 0, max: 10, step: 0.5 })}
        values={values}
        setValues={(fn) => {
          values = fn(values);
        }}
        mode="generate"
      />,
    );
    return {
      input: () => screen.getByRole("spinbutton", { name: "strength" }) as HTMLInputElement,
      values: () => values,
    };
  }

  it("slider 渲染可编辑的数字输入框并回写值", () => {
    const { input, values } = renderSlider("0.5");
    expect(input().value).toBe("0.5");
    fireEvent.change(input(), { target: { value: "3.5" } });
    expect(values()["8:seed"]).toBe("3.5");
  });

  it("slider 失焦时按 max 夹取", () => {
    const { input, values } = renderSlider("1");
    fireEvent.change(input(), { target: { value: "99" } });
    fireEvent.blur(input());
    expect(values()["8:seed"]).toBe("10");
  });

  it("slider 失焦时按 min 夹取", () => {
    const { input, values } = renderSlider("1");
    fireEvent.change(input(), { target: { value: "-5" } });
    fireEvent.blur(input());
    expect(values()["8:seed"]).toBe("0");
  });
});
