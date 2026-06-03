import { create } from "zustand";
import { persist } from "zustand/middleware";

interface AppState {
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
  detailCollapsed: boolean;
  toggleDetail: () => void;
  setDetailCollapsed: (v: boolean) => void;
}

export const useAppStore = create<AppState>()(
  persist(
    (set) => ({
      sidebarCollapsed: false,
      toggleSidebar: () =>
        set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
      detailCollapsed: false,
      toggleDetail: () =>
        set((state) => ({ detailCollapsed: !state.detailCollapsed })),
      setDetailCollapsed: (v) => set({ detailCollapsed: v }),
    }),
    {
      name: "medix-app-store",
      partialize: (state) => ({
        detailCollapsed: state.detailCollapsed,
      }),
    },
  ),
);
