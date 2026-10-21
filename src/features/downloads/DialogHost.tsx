import { useEffect, useState } from "react";
import { CalendarClock, FileCheck2, Gauge, PenLine, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/feedback";
import { Field, Input, Segmented } from "@/components/ui/inputs";
import { Dialog } from "@/components/ui/overlay";
import { attempt, useDialogs } from "@/lib/actions";
import { api } from "@/lib/api";
import { formatSpeed, parseRate } from "@/lib/format";
import { toast } from "@/lib/toast";

function toLocalInput(ms: number) {
  const d = new Date(ms - new Date().getTimezoneOffset() * 60000);
  return d.toISOString().slice(0, 16);
}

/** Small per-download dialogs opened from menus: rename, delete, speed limit, schedule, checksum. */
export function DialogHost() {
  const { kind, record, close } = useDialogs();
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [limitMode, setLimitMode] = useState<"none" | "custom">("none");

  useEffect(() => {
    if (!record || !kind) return;
    setBusy(false);
    setResult(null);
    if (kind === "rename") setText(record.fileName);
    if (kind === "limit") {
      setLimitMode(record.speedLimit ? "custom" : "none");
      setText(record.speedLimit ? `${Math.round(record.speedLimit / 1024)} KB` : "1 MB");
    }
    if (kind === "schedule") setText(toLocalInput(record.scheduledAt ?? Date.now() + 3600_000));
    if (kind === "checksum") setText(record.checksum ?? "");
  }, [kind, record]);

  if (!record) return null;
  const open = (k: typeof kind) => kind === k;

  const submitRename = async () => {
    const r = await attempt("Couldn't rename", () => api.rename(record.id, text));
    if (r) {
      toast.success("Renamed", r.fileName);
      close();
    }
  };

  const submitLimit = async () => {
    const value = limitMode === "none" ? null : parseRate(text);
    if (limitMode === "custom" && !value) {
      toast.error("Enter a speed such as 500 KB or 2 MB");
      return;
    }
    const ok = await attempt("Couldn't set the limit", () => api.setSpeedLimit(record.id, value));
    if (ok !== undefined) close();
  };

  const submitSchedule = async (clear = false) => {
    const at = clear ? null : new Date(text).getTime();
    if (!clear && (!at || Number.isNaN(at))) return;
    const ok = await attempt("Couldn't schedule", () => api.schedule(record.id, at));
    if (ok !== undefined) {
      toast.success(clear ? "Moved to the queue" : "Scheduled", clear ? undefined : new Date(at!).toLocaleString());
      close();
    }
  };

  const submitChecksum = async () => {
    setBusy(true);
    const actual = await attempt("Couldn't compute the checksum", () => api.verifyChecksum(record.id, text.trim() || null));
    setBusy(false);
    if (actual) {
      setResult(actual);
      if (text.trim()) {
        const ok = actual === text.trim().toLowerCase();
        if (ok) toast.success("Checksum matches");
        else toast.error("Checksum doesn't match", "The file may be corrupted or a different version.");
      }
    }
  };

  return (
    <>
      <Dialog
        open={open("rename")}
        onOpenChange={(o) => !o && close()}
        title="Rename download"
        icon={<PenLine size={20} className="mt-0.5 text-accent" />}
        footer={
          <>
            <Button variant="ghost" onClick={close}>Cancel</Button>
            <Button variant="primary" onClick={submitRename} disabled={!text.trim()}>Rename</Button>
          </>
        }
      >
        <Field label="File name" hint="The file on disk is renamed too. Characters Windows doesn't allow are replaced.">
          <Input autoFocus value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => e.key === "Enter" && submitRename()} />
        </Field>
      </Dialog>

      <Dialog
        open={open("remove")}
        onOpenChange={(o) => !o && close()}
        title="Remove from list?"
        description={record.status === "completed" ? "The downloaded file stays on your disk." : "Any partially downloaded data will be deleted."}
        width={440}
        footer={
          <>
            <Button variant="ghost" onClick={close}>Cancel</Button>
            <Button
              variant="primary"
              onClick={async () => {
                await attempt("Couldn't remove", () => api.remove([record.id]));
                close();
              }}
            >
              Remove
            </Button>
          </>
        }
      >
        <p className="truncate text-[13px] font-medium">{record.fileName}</p>
      </Dialog>

      <Dialog
        open={open("delete")}
        onOpenChange={(o) => !o && close()}
        title="Delete the downloaded file?"
        description="The file is moved to the Recycle Bin and removed from Velora's list."
        icon={<Trash2 size={20} className="mt-0.5 text-coral" />}
        width={460}
        footer={
          <>
            <Button variant="ghost" onClick={close}>Keep file</Button>
            <Button
              variant="danger"
              onClick={async () => {
                const ok = await attempt("Couldn't delete the file", () => api.deleteFile(record.id));
                if (ok !== undefined) toast.success("Moved to the Recycle Bin", record.fileName);
                close();
              }}
            >
              Delete file
            </Button>
          </>
        }
      >
        <p className="truncate font-mono text-[12px] text-muted" data-selectable>
          {record.destDir}\{record.fileName}
        </p>
      </Dialog>

      <Dialog
        open={open("limit")}
        onOpenChange={(o) => !o && close()}
        title="Speed limit for this download"
        description="Real throttling: Velora reads from the network more slowly. It applies on top of the global limit."
        icon={<Gauge size={20} className="mt-0.5 text-accent" />}
        width={460}
        footer={
          <>
            <Button variant="ghost" onClick={close}>Cancel</Button>
            <Button variant="primary" onClick={submitLimit}>Apply</Button>
          </>
        }
      >
        <div className="grid gap-4">
          <Segmented
            value={limitMode}
            onValueChange={setLimitMode}
            options={[
              { value: "none", label: "Unlimited" },
              { value: "custom", label: "Limit to…" },
            ]}
          />
          {limitMode === "custom" && (
            <Field label="Maximum speed" hint={parseRate(text) ? `= ${formatSpeed(parseRate(text))}` : "Examples: 500 KB, 2 MB, 1.5 MB"}>
              <Input autoFocus value={text} onChange={(e) => setText(e.target.value)} onKeyDown={(e) => e.key === "Enter" && submitLimit()} />
            </Field>
          )}
        </div>
      </Dialog>

      <Dialog
        open={open("schedule")}
        onOpenChange={(o) => !o && close()}
        title="Schedule download"
        description="Velora starts the download at this time. Schedules only run while Velora is open or in the tray."
        icon={<CalendarClock size={20} className="mt-0.5 text-violet" />}
        width={460}
        footer={
          <>
            {record.status === "scheduled" && (
              <Button variant="ghost" className="mr-auto" onClick={() => submitSchedule(true)}>
                Remove schedule
              </Button>
            )}
            <Button variant="ghost" onClick={close}>Cancel</Button>
            <Button variant="primary" onClick={() => submitSchedule()}>Schedule</Button>
          </>
        }
      >
        <Field label="Start at">
          <Input type="datetime-local" value={text} onChange={(e) => setText(e.target.value)} />
        </Field>
      </Dialog>

      <Dialog
        open={open("checksum")}
        onOpenChange={(o) => !o && close()}
        title="Verify SHA-256"
        description="Paste the checksum published by the source to confirm the file is intact, or leave it empty to just compute it."
        icon={<FileCheck2 size={20} className="mt-0.5 text-mint" />}
        width={520}
        footer={
          <>
            <Button variant="ghost" onClick={close}>Close</Button>
            <Button variant="primary" onClick={submitChecksum} disabled={busy}>
              {busy ? <Spinner size={14} className="text-on-accent" /> : null}
              {text.trim() ? "Verify" : "Compute"}
            </Button>
          </>
        }
      >
        <div className="grid gap-3">
          <Field label="Expected SHA-256 (optional)">
            <Input value={text} onChange={(e) => setText(e.target.value)} placeholder="64 hexadecimal characters" className="font-mono text-[12px]" />
          </Field>
          {result && (
            <div className="rounded-xl border border-line bg-hover p-3">
              <p className="text-[11.5px] font-semibold text-muted">Computed SHA-256</p>
              <p className="mt-1 font-mono text-[11.5px] break-all" data-selectable>
                {result}
              </p>
            </div>
          )}
        </div>
      </Dialog>
    </>
  );
}
