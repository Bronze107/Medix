import { create } from "zustand";

interface AppState {
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
  detailCollapsed: boolean;
  toggleDetail: () => void;
  setDetailCollapsed: (v: boolean) => void;
}

export const useAppStore = create<AppState>((set) => ({
  sidebarCollapsed: false,
  toggleSidebar: () => set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
  detailCollapsed: false,
  toggleDetail: () => set((state) => ({ detailCollapsed: !state.detailCollapsed })),
  setDetailCollapsed: (v) => set({ detailCollapsed: v }),
}));
