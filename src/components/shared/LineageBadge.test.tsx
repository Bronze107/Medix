import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { LineageBadge, formatLineageTitle } from "./LineageBadge";

describe("formatLineageTitle", () => {
  it("两侧都有时用 · 拼接", () => {
    expect(formatLineageTitle(3, 2)).toBe("衍生出 3 张图 · 源自 2 张图");
  });

  it("只有出边", () => {
    expect(formatLineageTitle(3, 0)).toBe("衍生出 3 张图");
  });

  it("只有入边", () => {
    expect(formatLineageTitle(0, 2)).toBe("源自 2 张图");
  });

  it("两侧皆无返回空串", () => {
    expect(formatLineageTitle(0, 0)).toBe("");
  });
});

describe("LineageBadge", () => {
  it("两侧都有时同时渲染两个数字", () => {
    render(<LineageBadge childCount={3} parentCount={2} />);
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByTitle("衍生出 3 张图 · 源自 2 张图")).toBeInTheDocument();
  });

  it("只有入边时只渲染入边", () => {
    render(<LineageBadge childCount={0} parentCount={2} />);
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.queryByText("3")).not.toBeInTheDocument();
    expect(screen.getByTitle("源自 2 张图")).toBeInTheDocument();
  });

  it("只有出边时只渲染出边", () => {
    render(<LineageBadge childCount={3} parentCount={0} />);
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByTitle("衍生出 3 张图")).toBeInTheDocument();
  });

  it("两侧皆无时不渲染任何内容", () => {
    const { container } = render(<LineageBadge childCount={0} parentCount={0} />);
    expect(container.firstChild).toBeNull();
  });

  it("传入 onActivate 时点击触发回调并阻止冒泡", () => {
    const onActivate = vi.fn();
    const parentClick = vi.fn();
    render(
      <div onClick={parentClick}>
        <LineageBadge childCount={3} parentCount={2} onActivate={onActivate} />
      </div>,
    );

    fireEvent.click(screen.getByRole("button"));

    expect(onActivate).toHaveBeenCalledTimes(1);
    expect(parentClick).not.toHaveBeenCalled();
  });

  it("未传 onActivate 时渲染为不可交互的 span", () => {
    render(<LineageBadge childCount={3} parentCount={0} />);
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});
