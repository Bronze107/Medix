import { describe, it, expect, beforeEach } from "vitest";
import { useAppStore } from "./appStore";
import { act } from "@testing-library/react";

// Reset store state before each test
beforeEach(() => {
  act(() => {
    useAppStore.setState({
      sidebarCollapsed: false,
      detailCollapsed: false,
      selectedMediaId: null,
    });
  });
});

describe("appStore", () => {
  it("has correct initial state", () => {
    const state = useAppStore.getState();
    expect(state.sidebarCollapsed).toBe(false);
    expect(state.detailCollapsed).toBe(false);
    expect(state.selectedMediaId).toBeNull();
  });

  it("toggleSidebar flips sidebarCollapsed", () => {
    const store = useAppStore.getState();

    act(() => store.toggleSidebar());
    expect(useAppStore.getState().sidebarCollapsed).toBe(true);

    act(() => useAppStore.getState().toggleSidebar());
    expect(useAppStore.getState().sidebarCollapsed).toBe(false);
  });

  it("toggleDetail flips detailCollapsed", () => {
    act(() => useAppStore.getState().toggleDetail());
    expect(useAppStore.getState().detailCollapsed).toBe(true);

    act(() => useAppStore.getState().toggleDetail());
    expect(useAppStore.getState().detailCollapsed).toBe(false);
  });

  it("setDetailCollapsed sets the value directly", () => {
    act(() => useAppStore.getState().setDetailCollapsed(true));
    expect(useAppStore.getState().detailCollapsed).toBe(true);

    act(() => useAppStore.getState().setDetailCollapsed(false));
    expect(useAppStore.getState().detailCollapsed).toBe(false);
  });

  it("setSelectedMediaId sets the selected media", () => {
    act(() => useAppStore.getState().setSelectedMediaId("media-123"));
    expect(useAppStore.getState().selectedMediaId).toBe("media-123");

    act(() => useAppStore.getState().setSelectedMediaId(null));
    expect(useAppStore.getState().selectedMediaId).toBeNull();
  });
});
