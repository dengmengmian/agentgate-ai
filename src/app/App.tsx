import { BrowserRouter, Routes, Route } from "react-router-dom";
import { I18nProvider } from "@/lib/i18n";
import { AppShell } from "@/components/layout/AppShell";
import { ToastContainer } from "@/components/common/Toast";
import { UpdateChecker } from "@/components/common/UpdateChecker";
import { Dashboard } from "@/pages/Dashboard";
import { Tools } from "@/pages/Tools";
import { Providers } from "@/pages/Providers";
import { Routes as RoutesPage } from "@/pages/Routes";
import { Gateway } from "@/pages/Gateway";
import { Logs } from "@/pages/Logs";
import { Diagnostics } from "@/pages/Diagnostics";
import { Settings } from "@/pages/Settings";
import { QuickSetup } from "@/pages/QuickSetup";

export function App() {
  return (
    <I18nProvider>
      <BrowserRouter>
        <Routes>
          <Route element={<AppShell />}>
            <Route path="/" element={<Dashboard />} />
            <Route path="/quick-setup" element={<QuickSetup />} />
            <Route path="/tools" element={<Tools />} />
            <Route path="/providers" element={<Providers />} />
            <Route path="/routes" element={<RoutesPage />} />
            <Route path="/gateway" element={<Gateway />} />
            <Route path="/logs" element={<Logs />} />
            <Route path="/diagnostics" element={<Diagnostics />} />
            <Route path="/settings" element={<Settings />} />
          </Route>
        </Routes>
        <ToastContainer />
        <UpdateChecker />
      </BrowserRouter>
    </I18nProvider>
  );
}
