import { HashRouter, Routes, Route, Navigate } from "react-router";
import Layout from "./components/Layout/Layout";
import AllMedia from "./components/AllMedia/AllMedia";
import Tags from "./components/Tags/Tags";
import Settings from "./components/Settings/Settings";
import Trash from "./components/Trash/Trash";

function App() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<Navigate to="/media" replace />} />
          <Route path="/media" element={<AllMedia />} />
          <Route path="/trash" element={<Trash />} />
          <Route path="/tags" element={<Tags />} />
          <Route path="/settings" element={<Settings />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}

export default App;
