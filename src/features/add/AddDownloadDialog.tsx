import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { AlertTriangle, CheckCircle2, ChevronDown, CircleHelp, FolderOpen, Link2, Plus, X } from "lucide-react";

import { Button, Kbd } from "@/components/ui/button";
import { Spinner } from "@/components/ui/feedback";
import { Field, Input, Segmented, Select, Textarea } from "@/components/ui/inputs";
import { Dialog, Tooltip } from "@/components/ui/overlay";
import { api, pickFolder, toError } from "@/lib/api";
import { cn } from "@/lib/cn";
import { CATEGORIES, FileIcon, categoryMeta } from "@/lib/fileTypes";
import { formatBytes, formatSpeed, hostOf, parseRate } from "@/lib/format";
import { useStore } from "@/lib/store";
import { toast } from "@/lib/toast";
import type { AddCheck, AddRequest, Category, Priority, ProbeInfo, ResolvedMedia, StartMode } from "@/types/engine";

type Probe = { state: "idle" } | { state: "loading" } | { state: "ok"; info: ProbeInfo } | { state: "error"; message: string };
type Media = { state: "idle" } | { state: "loading" } | { state: "none" } | { state: "ok"; media: ResolvedMedia } | { state: "error"; message: string };
type Step = { kind: "form" } | { kind: "duplicate"; check: AddCheck } | { kind: "conflict"; check: AddCheck };
type When = "now" | "queue" | "schedule";

export function extractUrls(text: string): string[] {
  const out: string[] = [];
  for (const token of text.split(/\s+/)) {
    const t = token.trim().replace(/^[<("']+|[>)"',.]+$/g, "");
    if (/^(https?:\/\/\S+|magnet:\?\S+)$/i.test(t) && !out.includes(t)) out.push(t);
  }
  return out;
}

function nameFromUrl(url: string) {
  if (/^magnet:/i.test(url)) return new URLSearchParams(url.slice(url.indexOf("?") + 1)).get("dn") || "Torrent";
  try {
    const last = new URL(url).pathname.split("/").filter(Boolean).pop();
    return last ? decodeURIComponent(last) : hostOf(url);
  } catch {
    return url;
  }
}

function defaultScheduleValue() {
  const d = new Date(Date.now() + 3600_000 - new Date().getTimezoneOffset() * 60000);
  return d.toISOString().slice(0, 16);
}

export function AddDownloadDialog() {
  const open = useStore((s) => s.addOpen);
  const prefill = useStore((s) => s.addPrefill);
  const close = useStore((s) => s.closeAdd);
  const settings = useStore((s) => s.settings);

  const [text, setText] = useState("");
  const [probe, setProbe] = useState<Probe>({ state: "idle" });
  const [fileName, setFileName] = useState("");
  const nameEdited = useRef(false);
  const setNameEdited = (v: boolean) => (nameEdited.current = v);
  const [destDir, setDestDir] = useState("");
  const [category, setCategory] = useState<Category | "auto">("auto");
  const [priority, setPriority] = useState<Priority>("normal");
  const [connections, setConnections] = useState(8);
  const [when, setWhen] = useState<When>("now");
  const [scheduleAt, setScheduleAt] = useState(defaultScheduleValue());
  const [advanced, setAdvanced] = useState(false);
  const [checksum, setChecksum] = useState("");
  const [limit, setLimit] = useState("");
  const [busy, setBusy] = useState(false);
  const [step, setStep] = useState<Step>({ kind: "form" });
  const [referrer, setReferrer] = useState<string | null>(null);
  const probeSeq = useRef(0);
  // A page with a known media resolver (e.g. an X post) is replaced by the video file it shows.
  const [media, setMedia] = useState<Media>({ state: "idle" });
  const [variant, setVariant] = useState(0);
  const mediaSeq = useRef(0);

  const urls = useMemo(() => extractUrls(text), [text]);
  const single = urls.length === 1 ? urls[0] : null;
  const batch = urls.length > 1;
  const resolved = media.state === "ok" ? media.media : null;
  const chosen = resolved?.variants[variant] ?? resolved?.variants[0] ?? null;
  const target = chosen?.url ?? single;
  const targetReferrer = resolved ? resolved.pageUrl : referrer;

  // Reset when opened.
  useEffect(() => {
    if (!open) return;
    setStep({ kind: "form" });
    setProbe({ state: "idle" });
    setFileName(prefill?.fileName ?? "");
    setNameEdited(Boolean(prefill?.fileName));
    setDestDir("");
    setCategory("auto");
    setPriority("normal");
    setConnections(settings?.defaultConnections ?? 8);
    setWhen("now");
    setScheduleAt(defaultScheduleValue());
    setAdvanced(false);
    setChecksum("");
    setLimit("");
    setBusy(false);
    setReferrer(prefill?.referrer ?? null);
    const initial = prefill?.urls?.join("\n") ?? prefill?.url ?? "";
    setText(initial);
    if (!initial) {
      // Like most download managers: offer a copied link straight away.
      api
        .readClipboard()
        .then((clip) => {
          const found = extractUrls(clip ?? "");
          if (found.length) setText(found.join("\n"));
        })
        .catch(() => {});
    }
  }, [open, prefill, settings?.defaultConnections]);

  // Step 1 (debounced): does a resolver recognise this page?
  useEffect(() => {
    if (!open || !single) {
      setMedia({ state: "idle" });
      return;
    }
    const seq = ++mediaSeq.current;
    setMedia({ state: "loading" });
    setVariant(0);
    const t = window.setTimeout(async () => {
      try {
        const m = await api.resolveMedia(single);
        if (seq !== mediaSeq.current) return;
        if (m) {
          setMedia({ state: "ok", media: m });
          setFileName((n) => (nameEdited.current && n ? n : m.variants[0].fileName));
        } else {
          setMedia({ state: "none" });
        }
      } catch (e) {
        if (seq === mediaSeq.current) setMedia({ state: "error", message: toError(e).message });
      }
    }, 420);
    return () => window.clearTimeout(t);
  }, [single, open]);

  // Step 2: inspect the file that will actually be downloaded.
  useEffect(() => {
    if (!open || !target || media.state === "idle" || media.state === "loading") {
      setProbe(media.state === "loading" ? { state: "loading" } : { state: "idle" });
      return;
    }
    if (media.state === "error") {
      setProbe({ state: "error", message: media.message });
      return;
    }
    const seq = ++probeSeq.current;
    if (media.state === "ok" && chosen?.format) {
      // yt-dlp downloads this one; the page itself isn't the file, so describe the chosen stream.
      const audio = chosen.mime.startsWith("audio/");
      setProbe({
        state: "ok",
        info: {
          url: target,
          finalUrl: target,
          fileName: chosen.fileName,
          host: media.media.source,
          size: chosen.size,
          mime: chosen.mime,
          resumable: true,
          etag: null,
          lastModified: null,
          contentEncoding: null,
          category: audio ? "audio" : "video",
        },
      });
      return;
    }
    const fromMedia = media.state === "ok";
    setProbe({ state: "loading" });
    (async () => {
      try {
        const info = await api.probe(target, targetReferrer);
        if (seq !== probeSeq.current) return;
        setProbe({ state: "ok", info });
        if (!fromMedia) setFileName((n) => (nameEdited.current && n ? n : info.fileName));
      } catch (e) {
        if (seq !== probeSeq.current) return;
        setProbe({ state: "error", message: toError(e).message });
        if (!fromMedia) setFileName((n) => (nameEdited.current && n ? n : nameFromUrl(target)));
      }
    })();
  }, [target, targetReferrer, open, media, chosen]);

  const chooseVariant = (index: number) => {
    setVariant(index);
    const v = resolved?.variants[index];
    if (v && !nameEdited.current) setFileName(v.fileName);
  };

  const info = probe.state === "ok" ? probe.info : null;
  const effectiveCategory: Category = category === "auto" ? (info?.category ?? "other") : category;
  const startMode = (): StartMode =>
    when === "now" ? { mode: "now" } : when === "queue" ? { mode: "queue" } : { mode: "schedule", at: new Date(scheduleAt).getTime() };

  const buildRequest = (url: string, extra: Partial<AddRequest> = {}): AddRequest => ({
    url,
    fileName: single ? fileName.trim() || null : null,
    destDir: destDir.trim() || null,
    category: category === "auto" ? (single ? effectiveCategory : null) : category,
    priority,
    start: startMode(),
    connections,
    speedLimit: limit.trim() ? parseRate(limit) || null : null,
    checksum: single ? checksum.trim() || null : null,
    referrer: single ? targetReferrer : referrer,
    mediaFormat: single ? (chosen?.format ?? null) : null,
    ...extra,
  });

  const finish = (count: number) => {
    const verb = when === "now" ? "Started" : when === "queue" ? "Added to queue" : "Scheduled";
    toast.success(count === 1 ? verb : `${verb}: ${count} downloads`, count === 1 && single ? fileName || nameFromUrl(single) : undefined);
    close();
  };

  const add = useCallback(
    async (overrides: Partial<AddRequest> = {}) => {
      if (!target) return;
      setBusy(true);
      try {
        await api.add(buildRequest(target, overrides));
        finish(1);
      } catch (e) {
        toast.error("Couldn't add the download", toError(e).message);
      } finally {
        setBusy(false);
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [target, targetReferrer, single, fileName, destDir, category, priority, connections, when, scheduleAt, checksum, limit, referrer, effectiveCategory, chosen],
  );

  const submit = async () => {
    if (busy || urls.length === 0) return;
    if (when === "schedule" && Number.isNaN(new Date(scheduleAt).getTime())) {
      toast.error("Choose a valid start time");
      return;
    }
    if (limit.trim() && !parseRate(limit)) {
      toast.error("Speed limit not understood", "Use values like 500 KB or 2 MB.");
      return;
    }
    if (batch) {
      setBusy(true);
      let added = 0;
      let skipped = 0;
      for (const link of urls) {
        try {
          // Pages such as X posts become their best video file.
          const m = await api.resolveMedia(link);
          const url = m ? m.variants[0].url : link;
          const check = await api.checkAdd(url, null, destDir || null, category === "auto" ? null : category);
          if (check.duplicateOf) {
            skipped++;
            continue;
          }
          await api.add(buildRequest(url, m ? { fileName: m.variants[0].fileName, referrer: m.pageUrl, mediaFormat: m.variants[0].format } : {}));
          added++;
        } catch (e) {
          toast.error(`Couldn't add ${nameFromUrl(link)}`, toError(e).message);
        }
      }
      setBusy(false);
      if (skipped) toast.info(`Skipped ${skipped} link${skipped === 1 ? "" : "s"} already downloading`);
      if (added) finish(added);
      return;
    }
    setBusy(true);
    try {
      if (media.state === "loading" || (resolved && !chosen)) return setBusy(false);
      if (media.state === "error") {
        setBusy(false);
        return toast.error("Couldn't get the video", media.message);
      }
      const check = await api.checkAdd(target!, fileName.trim() || null, destDir.trim() || null, effectiveCategory);
      setBusy(false);
      if (check.duplicateOf) return setStep({ kind: "duplicate", check });
      return resolveConflict(check);
    } catch (e) {
      setBusy(false);
      toast.error("Couldn't add the download", toError(e).message);
    }
  };

  const resolveConflict = (check: AddCheck) => {
    if (!check.fileExists && !check.conflictsWith) return add();
    // Never offer to overwrite a file another download is still writing.
    if (check.conflictsWith || settings?.overwritePolicy === "rename") return add({ fileName: check.suggestedName });
    if (settings?.overwritePolicy === "overwrite") return add({ overwrite: true });
    setStep({ kind: "conflict", check });
  };

  const browse = async () => {
    const dir = await pickFolder(destDir || settings?.defaultDir);
    if (dir) setDestDir(dir);
  };

  const resumeBadge = info ? (
    info.resumable ? (
      <span className="inline-flex items-center gap-1 rounded-full bg-[color-mix(in_oklab,var(--mint)_14%,transparent)] px-2 py-0.5 text-[11px] font-semibold text-mint">
        <CheckCircle2 size={12} /> Resume supported
      </span>
    ) : (
      <Tooltip content={info.contentEncoding ? `The server compresses this file (${info.contentEncoding}), so byte ranges can't be used.` : "The server didn't confirm byte-range support, so the download uses one connection and can't resume after pausing."}>
        <span className="inline-flex items-center gap-1 rounded-full bg-[color-mix(in_oklab,var(--amber)_14%,transparent)] px-2 py-0.5 text-[11px] font-semibold text-amber">
          <CircleHelp size={12} /> No resume
        </span>
      </Tooltip>
    )
  ) : null;

  const placeholderDir = settings?.categoryFolders?.[effectiveCategory] || (settings?.autoCategorize ? `${settings.defaultDir}\\${categoryMeta[effectiveCategory].label}` : settings?.defaultDir) || "Default folder";

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !o && close()}
      title={batch ? `Add ${urls.length} downloads` : "Add download"}
      width={600}
      icon={
        <div className="mt-0.5 grid size-9 place-items-center rounded-xl bg-[linear-gradient(135deg,var(--accent),var(--accent-2))] text-on-accent shadow-md">
          <Plus size={18} strokeWidth={2.5} />
        </div>
      }
      footer={
        step.kind === "form" ? (
          <>
            <span className="mr-auto hidden items-center gap-1.5 text-[11.5px] text-subtle sm:flex">
              <Kbd>Ctrl</Kbd>+<Kbd>Enter</Kbd> to add
            </span>
            <Button variant="ghost" onClick={close}>
              Cancel
            </Button>
            <Button variant="primary" onClick={submit} disabled={busy || urls.length === 0} data-testid="add-submit">
              {busy && <Spinner size={14} className="text-on-accent" />}
              {when === "now" ? "Download" : when === "queue" ? "Add to queue" : "Schedule"}
            </Button>
          </>
        ) : null
      }
    >
      <div
        className="grid gap-4"
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            submit();
          }
        }}
      >
        <AnimatePresence mode="wait" initial={false}>
          {step.kind === "duplicate" && (
            <motion.div key="dup" initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} className="rounded-2xl border border-[color-mix(in_oklab,var(--amber)_40%,transparent)] bg-[color-mix(in_oklab,var(--amber)_8%,transparent)] p-4">
              <p className="flex items-center gap-2 font-semibold text-amber">
                <AlertTriangle size={16} /> This link is already in your downloads
              </p>
              <p className="mt-1 text-[12.5px] text-muted">“{step.check.duplicateOf?.fileName}” is {step.check.duplicateOf?.status}. Add it again as a separate download?</p>
              <div className="mt-3 flex gap-2">
                <Button size="sm" variant="primary" onClick={() => resolveConflict(step.check)}>
                  Add anyway
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setStep({ kind: "form" })}>
                  Go back
                </Button>
              </div>
            </motion.div>
          )}
          {step.kind === "conflict" && (
            <motion.div key="conflict" initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} className="rounded-2xl border border-[color-mix(in_oklab,var(--amber)_40%,transparent)] bg-[color-mix(in_oklab,var(--amber)_8%,transparent)] p-4">
              <p className="flex items-center gap-2 font-semibold text-amber">
                <AlertTriangle size={16} /> A file with this name already exists
              </p>
              <p className="mt-1 text-[12.5px] text-muted break-all">
                {step.check.destDir}\{step.check.fileName}
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                <Button size="sm" variant="primary" onClick={() => add({ fileName: step.check.suggestedName })}>
                  Save as “{step.check.suggestedName}”
                </Button>
                <Button size="sm" variant="danger" onClick={() => add({ overwrite: true })}>
                  Replace existing file
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setStep({ kind: "form" })}>
                  Cancel
                </Button>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {step.kind === "form" && (
          <>
            <Field label="Download link" hint={batch ? "One download is created for each link." : "Paste one or more links. Press Ctrl+Enter to add."}>
              <Textarea
                autoFocus
                value={text}
                onChange={(e) => setText(e.target.value)}
                placeholder="https://example.com/file.zip"
                rows={batch ? 4 : 2}
                className="font-mono text-[12.5px]"
                spellCheck={false}
                data-testid="add-url"
              />
            </Field>

            {single && (
              <motion.div layout className="overflow-hidden rounded-2xl border border-line bg-[color-mix(in_oklab,var(--bg)_40%,var(--card))] p-3.5">
                <div className="flex items-center gap-3">
                  <FileIcon category={effectiveCategory} size={42} />
                  <div className="min-w-0 flex-1">
                    {probe.state === "loading" ? (
                      <div className="grid gap-2">
                        <div className="h-3.5 w-2/3 animate-pulse rounded bg-hover" />
                        <div className="h-3 w-1/3 animate-pulse rounded bg-hover" />
                      </div>
                    ) : (
                      <>
                        <p className="truncate text-[13.5px] font-semibold">{fileName || nameFromUrl(single)}</p>
                        <p className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[12px] text-muted">
                          <span>{info?.host ?? hostOf(single)}</span>
                          {info && (
                            <>
                              <span className="text-subtle">·</span>
                              <span className="tabular">{info.size != null ? formatBytes(info.size) : "Size unknown"}</span>
                              {info.mime && (
                                <>
                                  <span className="text-subtle">·</span>
                                  <span className="max-w-40 truncate">{info.mime.split(";")[0]}</span>
                                </>
                              )}
                            </>
                          )}
                        </p>
                      </>
                    )}
                  </div>
                  <div className="shrink-0">
                    {probe.state === "loading" && (
                      <span className="flex items-center gap-1.5 text-[11.5px] text-muted">
                        <Spinner size={13} /> Checking server
                      </span>
                    )}
                    {resumeBadge}
                  </div>
                </div>
                {resolved && (
                  <div className="mt-3 border-t border-line pt-3">
                    <p className="flex items-center gap-2 text-[12px] font-semibold">
                      <span className="rounded-md bg-[color-mix(in_oklab,var(--accent)_14%,transparent)] px-1.5 py-0.5 text-[11px] text-accent">
                        Video from {resolved.source}
                      </span>
                      {resolved.author && <span className="text-muted">@{resolved.author}</span>}
                    </p>
                    {resolved.title && <p className="mt-1.5 line-clamp-2 text-[12px] text-muted">{resolved.title}</p>}
                    <div className="mt-2.5 flex items-center gap-3">
                      <span className="text-[12px] font-semibold text-muted">Quality</span>
                      <Select
                        ariaLabel="Video quality"
                        value={String(variant)}
                        onValueChange={(v) => chooseVariant(Number(v))}
                        className="h-8.5 w-64"
                        options={resolved.variants.map((v, i) => ({
                          value: String(i),
                          label: `${v.label}${v.bitrate ? ` · ${(v.bitrate / 1_000_000).toFixed(1)} Mbps` : ""}${i === 0 ? " · best" : ""}`,
                        }))}
                      />
                    </div>
                  </div>
                )}
                {!resolved && info?.mime?.toLowerCase().startsWith("text/html") && (
                  <p className="mt-2.5 flex items-start gap-1.5 text-[12px] text-amber" data-testid="webpage-warning">
                    <AlertTriangle size={13} className="mt-0.5 shrink-0" />
                    <span>
                      This link opens a web page, not a file. Velora would save the page itself (HTML), not any video or file shown on it. To grab a file from a
                      page, use the Velora browser extension's “Find files and media”.
                    </span>
                  </p>
                )}
                {probe.state === "error" && (
                  <p className="mt-2.5 flex items-start gap-1.5 text-[12px] text-amber">
                    <AlertTriangle size={13} className="mt-0.5 shrink-0" />
                    <span>
                      {probe.message}
                      {media.state === "error" ? "" : " You can still add it; Velora will retry when it starts."}
                    </span>
                  </p>
                )}
              </motion.div>
            )}

            {batch && (
              <ul className="scroll-area grid max-h-44 gap-1.5">
                {urls.map((u) => (
                  <li key={u} className="flex items-center gap-2.5 rounded-xl border border-line bg-card px-3 py-2">
                    <Link2 size={14} className="shrink-0 text-subtle" />
                    <span className="min-w-0 flex-1 truncate text-[12.5px] font-medium">{nameFromUrl(u)}</span>
                    <span className="shrink-0 text-[11.5px] text-subtle">{hostOf(u)}</span>
                    <button className="text-subtle hover:text-coral" aria-label="Remove link" onClick={() => setText(urls.filter((x) => x !== u).join("\n"))}>
                      <X size={14} />
                    </button>
                  </li>
                ))}
              </ul>
            )}

            {urls.length > 0 && (
              <div className="grid grid-cols-2 gap-3.5">
                {single && (
                  <Field label="File name" className="col-span-2">
                    <Input
                      value={fileName}
                      onChange={(e) => {
                        setFileName(e.target.value);
                        setNameEdited(true);
                      }}
                      onKeyDown={(e) => e.key === "Enter" && submit()}
                      data-testid="add-filename"
                    />
                  </Field>
                )}
                <Field label="Save to" className="col-span-2">
                  <div className="flex gap-2">
                    <Input value={destDir} onChange={(e) => setDestDir(e.target.value)} placeholder={placeholderDir} className="font-mono text-[12px]" />
                    <Button variant="secondary" icon={<FolderOpen size={15} />} onClick={browse}>
                      Browse
                    </Button>
                  </div>
                </Field>
                <Field label="Category">
                  <Select<Category | "auto">
                    value={category}
                    onValueChange={setCategory}
                    options={[
                      { value: "auto", label: single ? `Automatic (${categoryMeta[effectiveCategory].label})` : "Automatic" },
                      ...CATEGORIES.map((c) => ({ value: c, label: categoryMeta[c].label })),
                    ]}
                  />
                </Field>
                <Field label="Priority">
                  <Select<Priority>
                    value={priority}
                    onValueChange={setPriority}
                    options={[
                      { value: "high", label: "High — front of the queue" },
                      { value: "normal", label: "Normal" },
                      { value: "low", label: "Low" },
                    ]}
                  />
                </Field>
                <Field label="When">
                  <Segmented<When>
                    value={when}
                    onValueChange={setWhen}
                    className="w-full"
                    options={[
                      { value: "now", label: "Now" },
                      { value: "queue", label: "Queue" },
                      { value: "schedule", label: "Later" },
                    ]}
                  />
                </Field>
                <Field
                  label="Connections"
                  hint={info && !info.resumable ? "This server allows only one connection." : "More isn't always faster; 8 suits most servers."}
                >
                  <Select
                    value={String(info && !info.resumable ? 1 : connections)}
                    disabled={Boolean(info && !info.resumable)}
                    onValueChange={(v) => setConnections(Number(v))}
                    options={[1, 2, 4, 6, 8, 12, 16].map((n) => ({ value: String(n), label: `${n} connection${n === 1 ? "" : "s"}` }))}
                  />
                </Field>
                <AnimatePresence initial={false}>
                  {when === "schedule" && (
                    <motion.div initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} exit={{ opacity: 0, height: 0 }} className="col-span-2 overflow-hidden">
                      <Field label="Start at" hint="Runs while Velora is open or in the tray.">
                        <Input type="datetime-local" value={scheduleAt} onChange={(e) => setScheduleAt(e.target.value)} />
                      </Field>
                    </motion.div>
                  )}
                </AnimatePresence>

                <button
                  className="col-span-2 flex items-center gap-1.5 text-[12px] font-semibold text-muted hover:text-fg"
                  onClick={() => setAdvanced((a) => !a)}
                  aria-expanded={advanced}
                >
                  <ChevronDown size={14} className={cn("transition-transform duration-200", advanced && "rotate-180")} />
                  Advanced
                </button>
                <AnimatePresence initial={false}>
                  {advanced && (
                    <motion.div initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} exit={{ opacity: 0, height: 0 }} className="col-span-2 overflow-hidden">
                      <div className="grid grid-cols-2 gap-3.5 pb-1">
                        <Field label="Speed limit" hint={parseRate(limit) ? `= ${formatSpeed(parseRate(limit))}` : "Empty = unlimited. e.g. 2 MB"}>
                          <Input value={limit} onChange={(e) => setLimit(e.target.value)} placeholder="Unlimited" />
                        </Field>
                        {single && (
                          <Field label="Expected SHA-256" hint="Checked after the download finishes.">
                            <Input value={checksum} onChange={(e) => setChecksum(e.target.value)} placeholder="Optional" className="font-mono text-[11.5px]" />
                          </Field>
                        )}
                      </div>
                    </motion.div>
                  )}
                </AnimatePresence>
              </div>
            )}
          </>
        )}
      </div>
    </Dialog>
  );
}
