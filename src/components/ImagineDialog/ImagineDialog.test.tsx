import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import ImagineDialog from "./ImagineDialog";
import type { ComfyWorkflowDetail, WorkflowParam } from "@/types/comfyui";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => p }));
vi.mock("@/hooks/useThumbnail", () => ({
  useThumbnail: (id: string | null) => (id ? `thumb://${id}` : null),
}));
vi.mock("@/hooks/usePromptHistory", () => ({
  usePromptHistory: () => ({ items: [], record: vi.fn(), clear: vi.fn() }),
}));
vi.mock("@/components/Toast/Toast", () => ({ showToast: vi.fn() }));

const { submitEdit, settingsGet, workflowList, workflowGet } = vi.hoisted(() => ({
  submitEdit: vi.fn(),
  settingsGet: vi.fn(),
  workflowList: vi.fn(),
  workflowGet: vi.fn(),
}));

vi.mock("@/lib/tauri", () => ({
  imageQueueSubmitEdit: submitEdit,
  settingsGet,
  comfyuiWorkflowList: workflowList,
  comfyuiWorkflowGet: workflowGet,
}));

function param(overrides: Partial<WorkflowParam>): WorkflowParam {
  return {
    node_id: "26",
    widget_name: "image",
    param_name: "26:image",
    label: "image",
    default_value: "default.png",
    field_type: "image_selector",
    order_index: 0,
    ...overrides,
  };
}

const workflow = {
  id: "w1",
  name: "Edit WF",
  workflow_type: "edit" as const,
  workflow_json: "",
  created_at: "",
  updated_at: "",
};

function mockComfy(params: WorkflowParam[]) {
  settingsGet.mockResolvedValue("comfyui");
  workflowList.mockResolvedValue([workflow]);
  const detail: ComfyWorkflowDetail = {
    ...workflow,
    params,
    result_nodes: [],
  };
  workflowGet.mockResolvedValue(detail);
}

function renderDialog(props: Partial<Parameters<typeof ImagineDialog>[0]> = {}) {
  return render(
    <ImagineDialog
      mediaId="m1"
      sourceMediaIds={["m1", "m2", "m3"]}
      onClose={vi.fn()}
      {...props}
    />,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  submitEdit.mockResolvedValue("task-1");
});

describe("ImagineDialog 多图模式", () => {
  it("渲染 N 张带序号的缩略图和计数文案，多图时不显示裁剪/蒙版", async () => {
    mockComfy([param({})]);
    renderDialog();

    await screen.findByText("已选择 3 张输入图");
    const imgs = document.querySelectorAll("img");
    expect(imgs.length).toBe(3);
    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "裁剪/蒙版" })).not.toBeInTheDocument();
  });

  it("工作流图片输入少于所选图时提示忽略", async () => {
    mockComfy([param({})]);
    renderDialog();

    expect(
      await screen.findByText("该工作流仅使用前 1 张图，其余 2 张将被忽略"),
    ).toBeInTheDocument();
  });

  it("单图但工作流图片输入更多时提示使用默认图", async () => {
    mockComfy([
      param({ node_id: "26", param_name: "26:image" }),
      param({ node_id: "33", param_name: "33:image" }),
    ]);
    renderDialog({ sourceMediaIds: ["m1"] });

    expect(
      await screen.findByText("该工作流有 2 个图片输入，多出的 1 个将使用默认图"),
    ).toBeInTheDocument();
    // 单图模式：裁剪/蒙版仍可用
    expect(await screen.findByRole("button", { name: "裁剪/蒙版" })).toBeInTheDocument();
  });

  it("非 ComfyUI 多图时提示仅使用第一张", async () => {
    settingsGet.mockResolvedValue("");
    renderDialog();

    expect(await screen.findByText("当前服务仅使用第一张参考图")).toBeInTheDocument();
  });

  it("ComfyUI 提交传完整源图数组，imageDataUrls 为 null（后端按 id 读原图）", async () => {
    mockComfy([
      param({ field_type: "image_selector", widget_name: "image", param_name: "26:image" }),
      param({
        field_type: "multiline",
        widget_name: "prompt",
        param_name: "6:prompt",
        default_value: "make it red",
      }),
    ]);
    renderDialog();

    const btn = await screen.findByRole("button", { name: "加入队列" });
    await waitFor(() => expect(btn).not.toBeDisabled());
    fireEvent.click(btn);

    await waitFor(() => expect(submitEdit).toHaveBeenCalledTimes(1));
    expect(submitEdit).toHaveBeenCalledWith(
      ["m1", "m2", "m3"],
      "make it red",
      expect.any(Object),
      "auto",
      "1k",
      1,
      "w1",
      null,
    );
  });

  it("非 ComfyUI 提交传完整源图数组，工作流参数为 undefined", async () => {
    settingsGet.mockResolvedValue("");
    renderDialog();

    const textarea = await screen.findByRole("textbox");
    fireEvent.change(textarea, { target: { value: "黑白素描风" } });
    fireEvent.click(screen.getByRole("button", { name: "加入队列" }));

    await waitFor(() => expect(submitEdit).toHaveBeenCalledTimes(1));
    expect(submitEdit).toHaveBeenCalledWith(
      ["m1", "m2", "m3"],
      "黑白素描风",
      undefined,
      "auto",
      "1k",
      1,
      null,
      null,
    );
  });
});

describe("ImagineDialog 单图模式回归", () => {
  it("单图仍显示单图预览与裁剪/蒙版按钮，不出现多图文案", async () => {
    mockComfy([param({})]);
    cleanup();
    renderDialog({ sourceMediaIds: ["m1"] });

    expect(await screen.findByRole("button", { name: "裁剪/蒙版" })).toBeInTheDocument();
    expect(screen.queryByText(/已选择/)).not.toBeInTheDocument();
  });
});
