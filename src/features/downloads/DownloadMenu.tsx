import {
  ArrowDownUp,
  CalendarClock,
  CircleSlash,
  Copy,
  ExternalLink,
  FileCheck2,
  FolderInput,
  FolderOpen,
  Gauge,
  Info,
  MoreHorizontal,
  Pause,
  PenLine,
  Play,
  RefreshCw,
  RotateCcw,
  Trash2,
  XCircle,
} from "lucide-react";

import { IconButton } from "@/components/ui/button";
import { Menu, MenuContent, MenuItem, MenuSeparator, MenuSub, MenuSubContent, MenuSubTrigger, MenuTrigger } from "@/components/ui/overlay";
import { actions, useDialogs } from "@/lib/actions";
import { CATEGORIES, categoryMeta } from "@/lib/fileTypes";
import { useStore } from "@/lib/store";
import { isRunning, type DownloadRecord, type Priority } from "@/types/engine";

/** Only the actions that make sense for the download's current state are shown. */
export function DownloadMenu({ record }: { record: DownloadRecord }) {
  const select = useStore((s) => s.select);
  const openDialog = useDialogs((s) => s.open);
  const running = isRunning(record.status);
  const done = record.status === "completed";
  const failed = record.status === "failed" || record.status === "cancelled";
  const waiting = record.status === "queued" || record.status === "scheduled" || record.status === "paused";

  return (
    <Menu>
      <MenuTrigger asChild>
        <IconButton label="More actions" tooltip={false} onClick={(e) => e.stopPropagation()}>
          <MoreHorizontal size={17} />
        </IconButton>
      </MenuTrigger>
      <MenuContent>
        <div onClick={(e) => e.stopPropagation()}>
          {running && <MenuItem icon={<Pause size={15} />} onSelect={() => actions.pause(record.id)}>Pause</MenuItem>}
          {waiting && <MenuItem icon={<Play size={15} />} onSelect={() => actions.resume(record.id)}>{record.status === "paused" ? "Resume" : "Start now"}</MenuItem>}
          {failed && <MenuItem icon={<RefreshCw size={15} />} onSelect={() => actions.resume(record.id)}>Retry</MenuItem>}
          {done && <MenuItem icon={<ExternalLink size={15} />} onSelect={() => actions.openFile(record.id)}>Open file</MenuItem>}
          <MenuItem icon={<FolderOpen size={15} />} onSelect={() => actions.showInFolder(record.id)}>Show in folder</MenuItem>
          <MenuItem icon={<Copy size={15} />} onSelect={() => actions.copyUrl(record)}>Copy original link</MenuItem>
          <MenuSeparator />
          {!running && <MenuItem icon={<PenLine size={15} />} onSelect={() => openDialog("rename", record)}>Rename…</MenuItem>}
          {!running && (
            <MenuSub>
              <MenuSubTrigger icon={<FolderInput size={15} />}>Move to category</MenuSubTrigger>
              <MenuSubContent>
                {CATEGORIES.map((c) => {
                  const Icon = categoryMeta[c].icon;
                  return (
                    <MenuItem key={c} icon={<Icon size={15} />} disabled={c === record.category} onSelect={() => actions.setCategory(record.id, c)}>
                      {categoryMeta[c].label}
                    </MenuItem>
                  );
                })}
              </MenuSubContent>
            </MenuSub>
          )}
          {(waiting || running) && (
            <MenuSub>
              <MenuSubTrigger icon={<ArrowDownUp size={15} />}>Priority</MenuSubTrigger>
              <MenuSubContent>
                {(["high", "normal", "low"] as Priority[]).map((p) => (
                  <MenuItem key={p} disabled={p === record.priority} onSelect={() => actions.setPriority(record.id, p)}>
                    {p[0].toUpperCase() + p.slice(1)}
                  </MenuItem>
                ))}
              </MenuSubContent>
            </MenuSub>
          )}
          {!done && <MenuItem icon={<Gauge size={15} />} onSelect={() => openDialog("limit", record)}>Speed limit…</MenuItem>}
          {(waiting || failed) && <MenuItem icon={<CalendarClock size={15} />} onSelect={() => openDialog("schedule", record)}>Schedule…</MenuItem>}
          {done && <MenuItem icon={<FileCheck2 size={15} />} onSelect={() => openDialog("checksum", record)}>Verify SHA-256…</MenuItem>}
          {(done || failed || record.status === "paused") && (
            <MenuItem icon={<RotateCcw size={15} />} onSelect={() => actions.restart(record.id)}>
              {done ? "Download again" : "Restart from scratch"}
            </MenuItem>
          )}
          <MenuItem icon={<Info size={15} />} onSelect={() => select(record.id)}>Properties</MenuItem>
          <MenuSeparator />
          {(running || waiting) && <MenuItem icon={<XCircle size={15} />} danger onSelect={() => actions.cancel(record.id)}>Cancel download</MenuItem>}
          <MenuItem icon={<CircleSlash size={15} />} onSelect={() => openDialog("remove", record)}>Remove from list</MenuItem>
          {done && <MenuItem icon={<Trash2 size={15} />} danger onSelect={() => openDialog("delete", record)}>Delete file…</MenuItem>}
        </div>
      </MenuContent>
    </Menu>
  );
}
