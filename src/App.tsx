import { useEffect } from "react";
import { HashRouter, Routes, Route, Navigate } from "react-router";
import Layout from "./components/Layout/Layout";
import AllMedia from "./components/AllMedia/AllMedia";
import CollectionView from "./components/AllMedia/CollectionView";
import CollectionsPage from "./components/CollectionsPage/CollectionsPage";
import Tags from "./components/Tags/Tags";
import Settings from "./components/Settings/Settings";
import Trash from "./components/Trash/Trash";

function App() {
  // Globally suppress browser context menu (desktop app behavior)
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (target.closest("[data-media-card]") || target.closest("[data-collection-card]")) return;
      e.preventDefault();
    };
    window.addEventListener("contextmenu", handler);
    return () => window.removeEventListener("contextmenu", handler);
  }, []);

  return (
    <HashRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<Navigate to="/media" replace />} />
          <Route path="/media" element={<AllMedia />} />
          <Route path="/collections" element={<CollectionsPage />} />
          <Route path="/collections/:id" element={<CollectionView />} />
          <Route path="/trash" element={<Trash />} />
          <Route path="/tags" element={<Tags />} />
          <Route path="/settings" element={<Settings />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}

export default App;
