import { AppWindow, Archive, File, FileText, Image, Music, Video, type LucideIcon } from "lucide-react";

import type { Category } from "@/types/engine";

export const categoryMeta: Record<Category, { label: string; icon: LucideIcon; color: string }> = {
  video: { label: "Videos", icon: Video, color: "var(--coral)" },
  audio: { label: "Audio", icon: Music, color: "var(--violet)" },
  image: { label: "Images", icon: Image, color: "var(--amber)" },
  document: { label: "Documents", icon: FileText, color: "var(--cyan)" },
  archive: { label: "Archives", icon: Archive, color: "#7aa2ff" },
  application: { label: "Applications", icon: AppWindow, color: "var(--mint)" },
  other: { label: "Other", icon: File, color: "var(--muted)" },
};

export const CATEGORIES: Category[] = ["video", "audio", "image", "document", "archive", "application", "other"];

export function FileIcon({ category, size = 40 }: { category: Category; size?: number }) {
  const meta = categoryMeta[category] ?? categoryMeta.other;
  const Icon = meta.icon;
  return (
    <div
      className="grid shrink-0 place-items-center rounded-xl"
      style={{
        width: size,
        height: size,
        color: meta.color,
        background: `color-mix(in oklab, ${meta.color} 14%, transparent)`,
        boxShadow: `inset 0 0 0 1px color-mix(in oklab, ${meta.color} 22%, transparent)`,
      }}
      aria-hidden
    >
      <Icon size={Math.round(size * 0.46)} strokeWidth={1.9} />
    </div>
  );
}
