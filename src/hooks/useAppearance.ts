import { useEffect, useState } from "react";

import type { Settings } from "@/types/engine";

function useMedia(query: string) {
  const [match, setMatch] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const m = window.matchMedia(query);
    const on = () => setMatch(m.matches);
    m.addEventListener("change", on);
    return () => m.removeEventListener("change", on);
  }, [query]);
  return match;
}

/** Apply theme, accent, density and motion preferences to <html> and report the effective values. */
export function useAppearance(settings: Settings | null) {
  const systemDark = useMedia("(prefers-color-scheme: dark)");
  const systemReduced = useMedia("(prefers-reduced-motion: reduce)");
  const theme = settings?.theme === "light" ? "light" : settings?.theme === "dark" ? "dark" : systemDark ? "dark" : "light";
  const reduced = settings?.reducedMotion === "on" ? true : settings?.reducedMotion === "off" ? false : systemReduced;

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = theme;
    root.dataset.accent = settings?.accent ?? "cyan";
    root.dataset.density = settings?.density ?? "comfortable";
    root.dataset.motion = reduced ? "reduced" : "full";
  }, [theme, reduced, settings?.accent, settings?.density]);

  // Pause decorative animation while the window is hidden or minimised.
  useEffect(() => {
    const on = () => (document.documentElement.dataset.hidden = String(document.hidden));
    on();
    document.addEventListener("visibilitychange", on);
    return () => document.removeEventListener("visibilitychange", on);
  }, []);

  return { theme, reduced };
}
