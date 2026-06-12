import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SearchBar from "./SearchBar"; // default export

describe("SearchBar", () => {
  it("renders search input and icon", () => {
    render(<SearchBar value="" onChange={vi.fn()} onClear={vi.fn()} />);
    expect(screen.getByPlaceholderText(/搜索/)).toBeInTheDocument();
  });

  it("shows clear button only when value is non-empty", () => {
    const { rerender } = render(
      <SearchBar value="" onChange={vi.fn()} onClear={vi.fn()} />,
    );
    // No clear button should be present when value is empty
    expect(screen.queryByRole("button")).toBeNull();

    rerender(
      <SearchBar value="test" onChange={vi.fn()} onClear={vi.fn()} />,
    );
    // Clear (X) button should now be visible
    const buttons = screen.getAllByRole("button");
    expect(buttons.length).toBeGreaterThan(0);
  });

  it("calls onClear when clear button is clicked", async () => {
    const onClear = vi.fn();
    render(<SearchBar value="test" onChange={vi.fn()} onClear={onClear} />);
    // The X button is the last button rendered
    const buttons = screen.getAllByRole("button");
    const clearBtn = buttons[buttons.length - 1];
    await userEvent.click(clearBtn);
    expect(onClear).toHaveBeenCalledTimes(1);
  });

  it("renders tag pill for tag: query", () => {
    render(
      <SearchBar value='tag:"black cat"' onChange={vi.fn()} onClear={vi.fn()} />,
    );
    expect(screen.getByText(/tag:/)).toBeInTheDocument();
    expect(screen.getByText("black cat")).toBeInTheDocument();
  });

  it("renders width/height pills with green color", () => {
    render(
      <SearchBar
        value="width:>1920 height:>1080"
        onChange={vi.fn()}
        onClear={vi.fn()}
      />,
    );
    expect(screen.getByText(/>1920/)).toBeInTheDocument();
    expect(screen.getByText(/>1080/)).toBeInTheDocument();
  });

  it("removes pill when X button is clicked", async () => {
    const onChange = vi.fn();
    render(
      <SearchBar
        value="width:>1920 height:>1080"
        onChange={onChange}
        onClear={vi.fn()}
      />,
    );
    // Click the X on the first pill
    const removeButtons = screen.getAllByRole("button");
    await userEvent.click(removeButtons[0]);
    expect(onChange).toHaveBeenCalledTimes(1);
    // The removed value should not contain the first pill
    const newVal = onChange.mock.calls[0][0];
    expect(newVal).not.toContain("width:>1920");
    expect(newVal).toContain("height:>1080");
  });

  it("removes quoted tag pill properly", async () => {
    const onChange = vi.fn();
    render(
      <SearchBar
        value='tag:"black cat" extra text'
        onChange={onChange}
        onClear={vi.fn()}
      />,
    );
    const removeButtons = screen.getAllByRole("button");
    await userEvent.click(removeButtons[0]); // remove the tag pill
    const newVal = onChange.mock.calls[0][0];
    expect(newVal).not.toContain('tag:"black cat"');
    expect(newVal).toBe("extra text");
  });

  it("shows mixed input with pills + plain text", () => {
    render(
      <SearchBar
        value="tag:cat width:>1000 some plain text"
        onChange={vi.fn()}
        onClear={vi.fn()}
      />,
    );
    // Pills should show, input should still contain the query
    expect(screen.getByText("cat")).toBeInTheDocument();
    expect(screen.getByText(/>1000/)).toBeInTheDocument();
    const input = screen.getByPlaceholderText(/搜索/);
    expect((input as HTMLInputElement).value).toBe(
      "tag:cat width:>1000 some plain text",
    );
  });

  it("calls onChange when typing", async () => {
    const onChange = vi.fn();
    render(<SearchBar value="" onChange={onChange} onClear={vi.fn()} />);
    const input = screen.getByPlaceholderText(/搜索/);
    await userEvent.type(input, "hello");
    expect(onChange).toHaveBeenCalled();
  });

  it("renders date pill for date: query", () => {
    render(
      <SearchBar
        value="date:2026-01-01..2026-06-30"
        onChange={vi.fn()}
        onClear={vi.fn()}
      />,
    );
    expect(screen.getByText(/date:/)).toBeInTheDocument();
  });

  it("renders size pill for size: query", () => {
    render(
      <SearchBar value="size:>1mb" onChange={vi.fn()} onClear={vi.fn()} />,
    );
    expect(screen.getByText(/size:/)).toBeInTheDocument();
    expect(screen.getByText(/>1mb/)).toBeInTheDocument();
  });

  it("shows no pills for plain text only", () => {
    const { container } = render(
      <SearchBar
        value="some plain search text"
        onChange={vi.fn()}
        onClear={vi.fn()}
      />,
    );
    const input = screen.getByPlaceholderText(/搜索/);
    expect((input as HTMLInputElement).value).toBe("some plain search text");
    // No pill elements should be rendered
    expect(container.querySelectorAll('[class*="rounded-md"]').length).toBe(0);
  });

  it("handles case-insensitive tag matching", () => {
    render(
      <SearchBar
        value='TAG:"Black Cat"'
        onChange={vi.fn()}
        onClear={vi.fn()}
      />,
    );
    expect(screen.getByText(/tag:/)).toBeInTheDocument();
    // parsePills lowercases the matched text, so label is "black cat" not "Black Cat"
    expect(screen.getByText("black cat")).toBeInTheDocument();
  });
});
