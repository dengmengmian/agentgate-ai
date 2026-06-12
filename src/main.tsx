import React, { Suspense, lazy } from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./app/App";
import "./index.css";

// 宠物窗口和主窗口共用一个 bundle、按 window label 分流——懒加载让
// 主窗口不用解析 PetApp 代码，宠物窗口也只多加载自己那个小 chunk。
const PetApp = lazy(() => import("./pet/PetApp").then((m) => ({ default: m.PetApp })));

// Apply saved theme (migrate "latte" → "light")
let savedTheme = localStorage.getItem("agentgate_theme");
if (savedTheme === "latte") { savedTheme = "light"; localStorage.setItem("agentgate_theme", "light"); }
document.documentElement.setAttribute("data-theme", savedTheme || "light");

const label = getCurrentWindow().label;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {label === "pet" ? (
      <Suspense fallback={null}>
        <PetApp />
      </Suspense>
    ) : (
      <App />
    )}
  </React.StrictMode>
);
