import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { TaskCard } from "./AiGenPage";
import type { ImageTaskInfo } from "@/lib/tauri";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => p }));

const PREVIEW = "data:image/jpeg;base64,AAAA";

function makeTask(overrides: Partial<ImageTaskInfo> = {}): ImageTaskInfo {
  return {
    task_id: "t1",
    task_type: "generate",
    prompt: "a cat",
    source_media_ids: null,
    status: "running",
    staged: [],
    error: null,
    created_at: "2026-01-01T00:00:00Z",
    progress: { value: 5, max: 25 },
    ...overrides,
  };
}

function renderCard(props: Partial<Parameters<typeof TaskCard>[0]> = {}) {
  const onPreview = vi.fn();
  render(
    <TaskCard
      task={makeTask()}
      onImport={() => {}}
      onDiscard={() => {}}
      onDismiss={() => {}}
      onPreview={onPreview}
      {...props}
    />,
  );
  return { onPreview };
}

describe("TaskCard 采样预览", () => {
  it("生成中渲染采样预览，点击把 data URL 交给放大回调", () => {
    const { onPreview } = renderCard({ preview: PREVIEW });
    const img = screen.getByRole("img", { name: "采样预览" });
    expect(img).toHaveAttribute("src", PREVIEW);

    fireEvent.click(img);
    expect(onPreview).toHaveBeenCalledWith(PREVIEW);
  });

  it("没有预览时不渲染预览图", () => {
    renderCard({ preview: null });
    expect(screen.queryByRole("img", { name: "采样预览" })).not.toBeInTheDocument();
  });

  it("任务结束后不再渲染预览（改显示结果图）", () => {
    renderCard({ preview: PREVIEW, task: makeTask({ status: "done" }) });
    expect(screen.queryByRole("img", { name: "采样预览" })).not.toBeInTheDocument();
  });
});

describe("TaskCard 状态", () => {
  const onCancel = vi.fn();

  it("queued 显示「排队中」而不是「生成中」（ComfyUI 里还没轮到它）", () => {
    renderCard({ task: makeTask({ status: "queued" }), onCancel, showCancel: true });
    expect(screen.getByText("排队中")).toBeInTheDocument();
    expect(screen.queryByText("生成中")).not.toBeInTheDocument();
  });

  it("queued 也能取消（走后端的出队路径），pending 不能（还没提交给 ComfyUI）", () => {
    const { unmount } = render(
      <TaskCard
        task={makeTask({ status: "queued" })}
        onImport={() => {}}
        onDiscard={() => {}}
        onDismiss={() => {}}
        onPreview={() => {}}
        onCancel={onCancel}
        showCancel
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(onCancel).toHaveBeenCalledWith("t1");
    unmount();

    render(
      <TaskCard
        task={makeTask({ status: "pending" })}
        onImport={() => {}}
        onDiscard={() => {}}
        onDismiss={() => {}}
        onPreview={() => {}}
        onCancel={onCancel}
        showCancel
      />,
    );
    expect(screen.queryByRole("button", { name: "取消" })).not.toBeInTheDocument();
  });
});

describe("TaskCard 视频产物", () => {
  const videoStaged: ImageTaskInfo["staged"][number] = {
    id: "v1",
    path: "C:/staging/v1.mp4",
    width: 832,
    height: 480,
    file_size: 123456,
    media_type: "video",
    duration: 5,
  };
  const imageStaged: ImageTaskInfo["staged"][number] = {
    id: "i1",
    path: "C:/staging/i1.png",
    width: 1024,
    height: 768,
    file_size: 123456,
    media_type: "image",
    duration: null,
  };

  it("视频产物渲染 <video>、「视频」徽标和时长标签，双击预览带 isVideo=true", () => {
    const { onPreview } = renderCard({
      task: makeTask({ status: "done", staged: [videoStaged] }),
    });
    const video = document.querySelector("video");
    expect(video).toBeInTheDocument();
    expect(video).toHaveAttribute("src", videoStaged.path);
    expect(screen.getByText("视频")).toBeInTheDocument();
    expect(screen.getByText("5.0s")).toBeInTheDocument();

    fireEvent.dblClick(video!);
    expect(onPreview).toHaveBeenCalledWith(videoStaged.path, true);
  });

  it("图片产物仍渲染 <img> 和尺寸标签，双击预览 isVideo 为假值", () => {
    const { onPreview } = renderCard({
      task: makeTask({ status: "done", staged: [imageStaged] }),
    });
    expect(document.querySelector("video")).not.toBeInTheDocument();
    const img = document.querySelector("img");
    expect(img).toHaveAttribute("src", imageStaged.path);
    expect(screen.getByText("1024×768")).toBeInTheDocument();

    fireEvent.dblClick(img);
    expect(onPreview).toHaveBeenCalledWith(imageStaged.path, false);
  });
});
