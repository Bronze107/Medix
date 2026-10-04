import { describe, it, expect, beforeEach } from "vitest";
import {
  clearWorkflowValues,
  defaultWorkflowValues,
  mergeWorkflowValues,
  rememberWorkflowValues,
} from "./workflowValueMemory";
import type { WorkflowParam } from "@/types/comfyui";

const KEY = "medix.comfyuiWorkflowValues";

function param(overrides: Partial<WorkflowParam>): WorkflowParam {
  return {
    node_id: "8",
    widget_name: "seed",
    param_name: "8:seed",
    label: "seed",
    default_value: "0",
    field_type: "number",
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

const textParam = (nodeId: string, name: string, def: string) =>
  param({ node_id: nodeId, widget_name: name, param_name: `${nodeId}:${name}`, label: name, default_value: def, field_type: "text" });

const imageParam = () =>
  param({
    node_id: "28",
    widget_name: "image",
    param_name: "28:image",
    label: "image",
    default_value: "default.png",
    field_type: "image_selector",
  });

beforeEach(() => {
  // test-setup.ts 只做 jest-dom + cleanup，不会在用例之间清 localStorage
  localStorage.clear();
});

describe("workflowValueMemory", () => {
  it("remember 后 merge 取回上次的值", () => {
    const params = [param({})];
    rememberWorkflowValues("wf1", params, { "8:seed": "42" });
    expect(mergeWorkflowValues("wf1", params)["8:seed"]).toBe("42");
  });

  it("merge 以工作流默认值为底，覆盖记忆值，并丢弃已删除的参数", () => {
    const before = [param({}), textParam("9", "text", "默认文本")];
    rememberWorkflowValues("wf1", before, {
      "8:seed": "42",
      "9:text": "上次的文本",
    });
    // 工作流被改过：9:text 已不存在
    const after = [param({})];
    const merged = mergeWorkflowValues("wf1", after);
    expect(merged["8:seed"]).toBe("42");
    expect(merged["9:text"]).toBeUndefined();
    expect(Object.keys(merged)).toEqual(["8:seed"]);
  });

  it("image_selector 永不记忆，始终用工作流默认值", () => {
    const params = [imageParam()];
    rememberWorkflowValues("wf1", params, { "28:image": "uploaded_xxx.png" });
    expect(mergeWorkflowValues("wf1", params)["28:image"]).toBe("default.png");
    // 也不该写进存储
    expect(localStorage.getItem(KEY) ?? "").not.toContain("uploaded_xxx.png");
  });

  it("未记忆过的工作流返回默认值", () => {
    const params = [textParam("9", "text", "默认文本")];
    expect(defaultWorkflowValues(params)).toEqual({ "9:text": "默认文本" });
    expect(mergeWorkflowValues("never-seen", params)).toEqual({ "9:text": "默认文本" });
  });

  it("存储内容损坏时不抛异常，回落默认值", () => {
    const params = [param({})];
    localStorage.setItem(KEY, "not json at all");
    expect(mergeWorkflowValues("wf1", params)["8:seed"]).toBe("0");

    localStorage.setItem(KEY, JSON.stringify(["unexpected", "array"]));
    expect(mergeWorkflowValues("wf1", params)["8:seed"]).toBe("0");
  });

  it("clearWorkflowValues 只清目标工作流", () => {
    const params = [param({})];
    rememberWorkflowValues("wf1", params, { "8:seed": "1" });
    rememberWorkflowValues("wf2", params, { "8:seed": "2" });

    clearWorkflowValues("wf1");

    expect(mergeWorkflowValues("wf1", params)["8:seed"]).toBe("0");
    expect(mergeWorkflowValues("wf2", params)["8:seed"]).toBe("2");
  });
});
