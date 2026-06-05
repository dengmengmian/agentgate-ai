import { useEffect } from "react";
import { BrowserRouter, Routes, Route, useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import { I18nProvider } from "@/lib/i18n";
import { AppShell } from "@/components/layout/AppShell";
import { ToastContainer } from "@/components/common/Toast";
import { UpdateChecker } from "@/components/common/UpdateChecker";
import { Dashboard } from "@/pages/Dashboard";
import { Tools } from "@/pages/Tools";
import { Providers } from "@/pages/Providers";
import { ProviderDetail } from "@/pages/ProviderDetail";
import { Routes as RoutesPage } from "@/pages/Routes";
import { Gateway } from "@/pages/Gateway";
import { Logs } from "@/pages/Logs";
import { Diagnostics } from "@/pages/Diagnostics";
import { Settings } from "@/pages/Settings";
import { QuickSetup } from "@/pages/QuickSetup";
import { Instructions } from "@/pages/Instructions";
import { Mcp } from "@/pages/Mcp";

/// 宠物右键菜单会发页面导航事件,这里统一跳过去。
function PetEventBridge() {
  const navigate = useNavigate();
  useEffect(() => {
    const unSettings = listen("pet-open-settings", () => navigate("/settings?tab=pet"));
    const unGateway = listen("pet-open-gateway", () => navigate("/gateway"));
    const unLogs = listen("pet-open-logs", () => navigate("/logs?source=gateway"));
    return () => {
      unSettings.then((fn) => fn());
      unGateway.then((fn) => fn());
      unLogs.then((fn) => fn());
    };
  }, [navigate]);
  return null;
}

export function App() {
  return (
    <I18nProvider>
      <BrowserRouter>
        <PetEventBridge />
        <Routes>
          <Route element={<AppShell />}>
            <Route path="/" element={<Dashboard />} />
            <Route path="/quick-setup" element={<QuickSetup />} />
            <Route path="/tools" element={<Tools />} />
            <Route path="/providers" element={<Providers />} />
            <Route path="/providers/:id" element={<ProviderDetail />} />
            <Route path="/routes" element={<RoutesPage />} />
            <Route path="/gateway" element={<Gateway />} />
            <Route path="/logs" element={<Logs />} />
            <Route path="/diagnostics" element={<Diagnostics />} />
            <Route path="/instructions" element={<Instructions />} />
            <Route path="/mcp" element={<Mcp />} />
            <Route path="/settings" element={<Settings />} />
          </Route>
        </Routes>
        <ToastContainer />
        <UpdateChecker />
      </BrowserRouter>
    </I18nProvider>
  );
}
