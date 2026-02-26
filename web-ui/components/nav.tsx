"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

const items = [
  { href: "/", label: "Overview", icon: "overview" },
  { href: "/live", label: "Live", icon: "live" },
  { href: "/trends", label: "Trends", icon: "trends" },
  { href: "/trends-accum", label: "Accum Trends", icon: "accum" },
  { href: "/history", label: "History", icon: "history" },
  { href: "/commands", label: "Commands", icon: "commands" },
  { href: "/audit", label: "Audit", icon: "audit" },
];

function SidebarIcon({ name }: { name: string }) {
  const common = { width: 16, height: 16, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 2 };
  if (name === "overview") {
    return (
      <svg {...common}>
        <rect x="3" y="3" width="8" height="8" />
        <rect x="13" y="3" width="8" height="5" />
        <rect x="13" y="10" width="8" height="11" />
        <rect x="3" y="13" width="8" height="8" />
      </svg>
    );
  }
  if (name === "live") {
    return (
      <svg {...common}>
        <circle cx="12" cy="12" r="9" />
        <circle cx="12" cy="12" r="3" />
      </svg>
    );
  }
  if (name === "trends") {
    return (
      <svg {...common}>
        <path d="M3 17l6-6 4 4 8-8" />
      </svg>
    );
  }
  if (name === "accum") {
    return (
      <svg {...common}>
        <path d="M4 16h16" />
        <path d="M4 12h12" />
        <path d="M4 8h8" />
      </svg>
    );
  }
  if (name === "history") {
    return (
      <svg {...common}>
        <path d="M3 12a9 9 0 109-9" />
        <path d="M3 3v6h6" />
        <path d="M12 8v5l3 2" />
      </svg>
    );
  }
  if (name === "commands") {
    return (
      <svg {...common}>
        <path d="M4 8l4 4-4 4" />
        <path d="M12 16h8" />
      </svg>
    );
  }
  return (
    <svg {...common}>
      <path d="M12 2l8 4v6c0 5-3.5 8-8 10-4.5-2-8-5-8-10V6l8-4z" />
    </svg>
  );
}

export function Nav({ collapsed, onToggle }: { collapsed: boolean; onToggle: () => void }) {
  const pathname = usePathname();
  return (
    <aside className={`nav ${collapsed ? "collapsed" : ""}`}>
      <div className="nav-head">
        {!collapsed && <h1>IFASCADA HMI</h1>}
        <button className="nav-toggle" onClick={onToggle} title={collapsed ? "Expand" : "Collapse"}>
          {collapsed ? "»" : "«"}
        </button>
      </div>
      {items.map((item) => {
        const active = pathname === item.href;
        return (
          <Link key={item.href} href={item.href} className={`item ${active ? "active" : ""}`}>
            <span className="item-inner">
              <span className="item-icon" aria-hidden>
                <SidebarIcon name={item.icon} />
              </span>
              {!collapsed && <span className="item-label">{item.label}</span>}
            </span>
          </Link>
        );
      })}
    </aside>
  );
}
