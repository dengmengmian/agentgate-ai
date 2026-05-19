import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./app/App";
import { PetApp } from "./pet/PetApp";
import "./index.css";

// Apply saved theme (migrate "latte" → "light")
let savedTheme = localStorage.getItem("agentgate_theme");
if (savedTheme === "latte") { savedTheme = "light"; localStorage.setItem("agentgate_theme", "light"); }
if (savedTheme) document.documentElement.setAttribute("data-theme", savedTheme);

const label = getCurrentWindow().label;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {label === "pet" ? <PetApp /> : <App />}
  </React.StrictMode>
);
