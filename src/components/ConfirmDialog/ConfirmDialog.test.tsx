import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ConfirmDialog } from "./ConfirmDialog";

describe("ConfirmDialog", () => {
  it("renders title and message when open", () => {
    render(
      <ConfirmDialog
        open={true}
        title="Delete Item"
        message="Are you sure?"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText("Delete Item")).toBeInTheDocument();
    expect(screen.getByText("Are you sure?")).toBeInTheDocument();
  });

  it("does not render when open is false", () => {
    const { container } = render(
      <ConfirmDialog
        open={false}
        title="Test"
        message="msg"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(container.firstChild).toBeNull();
  });

  it("calls onConfirm when confirm button is clicked", async () => {
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog
        open={true}
        title="Test"
        message="msg"
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );
    await userEvent.click(screen.getByText("确定"));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("calls onCancel when cancel button is clicked", async () => {
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        open={true}
        title="Test"
        message="msg"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />,
    );
    await userEvent.click(screen.getByText("取消"));
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("calls onCancel when backdrop is clicked", async () => {
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        open={true}
        title="Test"
        message="msg"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />,
    );
    // Click the backdrop (fixed overlay) — the outer div
    await userEvent.click(
      document.querySelector(".fixed.inset-0") as HTMLElement,
    );
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("uses custom button labels", () => {
    render(
      <ConfirmDialog
        open={true}
        title="Test"
        message="msg"
        confirmLabel="Yes"
        cancelLabel="No"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText("Yes")).toBeInTheDocument();
    expect(screen.getByText("No")).toBeInTheDocument();
  });

  it("renders danger variant with correct styling", () => {
    render(
      <ConfirmDialog
        open={true}
        title="Delete"
        message="Cannot undo"
        variant="danger"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    const confirmBtn = screen.getByText("确定");
    expect(confirmBtn.className).toContain("var(--color-danger)");
  });

  it("clicking inside dialog does not trigger onCancel", async () => {
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        open={true}
        title="Test"
        message="msg"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />,
    );
    // Click the inner dialog card (should be stopped by stopPropagation)
    await userEvent.click(screen.getByText("Test"));
    expect(onCancel).not.toHaveBeenCalled();
  });
});
