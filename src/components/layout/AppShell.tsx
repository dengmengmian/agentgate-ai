import { useState, useEffect, Suspense } from "react";
import { Outlet } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { Topbar } from "./Topbar";
import { CommandPalette } from "@/components/common/CommandPalette";

export function AppShell() {
  // Cmd+K（mac）/ Ctrl+K（其它平台）全局快捷键打开命令面板。
  const [cmdkOpen, setCmdkOpen] = useState(false);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setCmdkOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-bg">
      <Sidebar />
      <div className="flex flex-1 flex-col overflow-hidden">
        <Topbar onOpenCmdK={() => setCmdkOpen(true)} />
        <main className="flex-1 overflow-y-auto scroll-smooth p-6">
          <div className="animate-fade-in">
            {/* 懒加载页面 chunk 时只替换内容区，侧边栏 / 顶栏保持不动。
                本地磁盘加载 chunk 是毫秒级，骨架几乎不可见，仅防白屏。 */}
            <Suspense fallback={<div className="skeleton h-40 w-full" />}>
              <Outlet />
            </Suspense>
          </div>
        </main>
      </div>
      <CommandPalette open={cmdkOpen} onClose={() => setCmdkOpen(false)} />
    </div>
  );
}
