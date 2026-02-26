"use client";

import { ReactNode, useEffect, useState } from "react";
import { Nav } from "@/components/nav";
import { ContextBar } from "@/components/context-bar";

export function AppShell({ children }: { children: ReactNode }) {
  const [collapsed, setCollapsed] = useState(false);

  useEffect(() => {
    try {
      const raw = window.localStorage.getItem("hmi.nav.collapsed");
      if (raw === "1") setCollapsed(true);
    } catch {}
  }, []);

  useEffect(() => {
    try {
      window.localStorage.setItem("hmi.nav.collapsed", collapsed ? "1" : "0");
    } catch {}
  }, [collapsed]);

  return (
    <div className={`shell ${collapsed ? "nav-collapsed" : ""}`}>
      <Nav collapsed={collapsed} onToggle={() => setCollapsed((v) => !v)} />
      <main className="content">
        <ContextBar />
        {children}
      </main>
    </div>
  );
}
