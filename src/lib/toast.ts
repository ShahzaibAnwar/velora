import { create } from "zustand";

export interface Toast {
  id: number;
  tone: "success" | "error" | "info";
  title: string;
  description?: string;
  action?: { label: string; run: () => void };
  duration: number;
}

interface ToastState {
  toasts: Toast[];
  push: (t: Omit<Toast, "id" | "duration"> & { duration?: number }) => number;
  dismiss: (id: number) => void;
}

let nextId = 1;

export const useToasts = create<ToastState>((set, get) => ({
  toasts: [],
  push: (t) => {
    const id = nextId++;
    const toast: Toast = { duration: t.action ? 8000 : 4200, ...t, id };
    set({ toasts: [...get().toasts.slice(-3), toast] });
    window.setTimeout(() => get().dismiss(id), toast.duration);
    return id;
  },
  dismiss: (id) => set({ toasts: get().toasts.filter((t) => t.id !== id) }),
}));

export const toast = {
  success: (title: string, description?: string) => useToasts.getState().push({ tone: "success", title, description }),
  error: (title: string, description?: string) => useToasts.getState().push({ tone: "error", title, description }),
  info: (title: string, description?: string, action?: Toast["action"]) =>
    useToasts.getState().push({ tone: "info", title, description, action }),
};
