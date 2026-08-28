import { useEffect, useState } from "react";

export type ResponsiveHeaderTier = "mobile" | "tablet" | "desktop";

export function getResponsiveHeaderTier(width: number): ResponsiveHeaderTier {
  if (width >= 1024) return "desktop";
  if (width >= 768) return "tablet";
  return "mobile";
}

function readTier(): ResponsiveHeaderTier {
  if (typeof window === "undefined") return "desktop";
  if (window.matchMedia("(min-width: 1024px)").matches) return "desktop";
  if (window.matchMedia("(min-width: 768px)").matches) return "tablet";
  return "mobile";
}

export function useResponsiveHeaderTier(): ResponsiveHeaderTier {
  const [tier, setTier] = useState<ResponsiveHeaderTier>(readTier);

  useEffect(() => {
    const tablet = window.matchMedia("(min-width: 768px)");
    const desktop = window.matchMedia("(min-width: 1024px)");
    const update = () => {
      setTier(readTier());
    };
    tablet.addEventListener("change", update);
    desktop.addEventListener("change", update);
    update();
    return () => {
      tablet.removeEventListener("change", update);
      desktop.removeEventListener("change", update);
    };
  }, []);

  return tier;
}
