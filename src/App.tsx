import { useEffect, useState } from "react";
import { HashRouter, Route, Routes, useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import Sidebar from "./components/Sidebar";
import Dashboard from "./pages/dashboard";
import NewAnalysis from "./pages/new-analysis";
import ReportPage from "./pages/report";
import MyReports from "./pages/reports";
import Screener from "./pages/screener";
import Watchlists from "./pages/watchlists";
import Compare from "./pages/compare";
import Settings from "./pages/settings";
import Help from "./pages/help";
import DbBrowser from "./pages/db-browser";
import { getDbHealth } from "./lib/api";
import { AnalysisQueueProvider } from "./contexts/AnalysisQueueContext";

const DEV = import.meta.env.DEV;
const inTauri = "__TAURI_INTERNALS__" in window;

function Shell() {
  const navigate = useNavigate();
  const [health, setHealth] = useState<string>("connecting…");
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    getDbHealth()
      .then((h) =>
        setHealth(
          `backend ok · ${h.tables} tables · ${h.countries} countries · ${h.prompts} prompts`,
        ),
      )
      .catch(() => setHealth("backend unavailable (run via `tauri dev`)"));
  }, []);

  useEffect(() => {
    if (!inTauri) return;
    const disposed: Array<() => void> = [];
    listen<string>("menu://nav", (e) => {
      navigate(e.payload);
    }).then((unlisten) => disposed.push(unlisten));
    return () => {
      disposed.forEach((fn) => fn());
    };
  }, [navigate]);

  return (
    <div className="flex h-screen w-screen bg-app-bg">
      <Sidebar footer={health} />
      <main className="relative flex-1 overflow-y-auto">
        {notice && (
          <div className="flex items-center justify-between gap-4 border-b border-app-border bg-app-warning/10 px-6 py-2 text-sm text-app-text">
            <span>{notice}</span>
            <button
              type="button"
              onClick={() => setNotice(null)}
              className="text-app-muted hover:text-app-text"
            >
              Dismiss
            </button>
          </div>
        )}
        <div className="p-6">
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/new-analysis" element={<NewAnalysis />} />
            <Route path="/report/:ticker" element={<ReportPage />} />
            <Route path="/compare/:ticker" element={<Compare />} />
            <Route path="/reports" element={<MyReports />} />
            <Route path="/screener" element={<Screener />} />
            <Route path="/watchlists" element={<Watchlists />} />
            <Route path="/settings" element={<Settings />} />
            <Route path="/help" element={<Help />} />
            {DEV && <Route path="/db-browser" element={<DbBrowser />} />}
          </Routes>
        </div>
      </main>
    </div>
  );
}

export default function App() {
  return (
    <HashRouter>
      <AnalysisQueueProvider>
        <Shell />
      </AnalysisQueueProvider>
    </HashRouter>
  );
}