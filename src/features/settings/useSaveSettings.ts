import { useCallback } from "react";

import { api, toError } from "@/lib/api";
import { useStore } from "@/lib/store";
import { toast } from "@/lib/toast";
import type { Settings } from "@/types/engine";

/**
 * Apply a settings change optimistically and persist it through the engine, which validates and
 * normalises it. On failure the previous settings are restored.
 */
export function useSaveSettings() {
  return useCallback(async (next: Settings) => {
    const { settings: prev, setSettings } = useStore.getState();
    setSettings(next);
    try {
      setSettings(await api.updateSettings(next));
    } catch (e) {
      if (prev) setSettings(prev);
      toast.error("Setting not saved", toError(e).message);
    }
  }, []);
}
