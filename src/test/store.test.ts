import { beforeEach, describe, expect, it } from "vitest";

import { matchesView, useStore } from "@/lib/store";
import type { DownloadRecord } from "@/types/engine";

function record(partial: Partial<DownloadRecord>): DownloadRecord {
  return {
    id: "a",
    url: "https://example.com/a.zip",
    finalUrl: null,
    fileName: "a.zip",
    nameAuto: false,
    destDir: "C:\\Downloads",
    destAuto: true,
    category: "archive",
    status: "queued",
    priority: "normal",
    queuePos: 0,
    totalSize: 100,
    downloaded: 0,
    resumable: true,
    connections: 8,
    speedLimit: null,
    etag: null,
    lastModified: null,
    mime: null,
    referrer: null,
    error: null,
    createdAt: 1,
    startedAt: null,
    completedAt: null,
    scheduledAt: null,
    checksum: null,
    checksumState: "none",
    overwrite: false,
    restartPending: false,
    ...partial,
  };
}

describe("store", () => {
  beforeEach(() => useStore.setState({ downloads: {}, progress: {}, notices: [], unreadNotices: 0, justCompleted: {} }));

  it("applies upserts and progress, and drops progress when a download stops", () => {
    const s = useStore.getState();
    s.applyEvent({ type: "upserted", record: record({ status: "downloading" }) });
    s.applyEvent({ type: "progress", items: [{ id: "a", downloaded: 40, totalSize: 100, speed: 10, etaSecs: 6, connections: 4 }], totalSpeed: 10 });
    expect(useStore.getState().progress.a.downloaded).toBe(40);
    s.applyEvent({ type: "upserted", record: record({ status: "paused", downloaded: 40 }) });
    expect(useStore.getState().progress.a).toBeUndefined();
    expect(useStore.getState().downloads.a.status).toBe("paused");
  });

  it("records completion notices and the celebration marker", () => {
    const s = useStore.getState();
    const r = record({ status: "completed" });
    s.applyEvent({ type: "upserted", record: r });
    s.applyEvent({ type: "completed", record: r });
    const st = useStore.getState();
    expect(st.notices).toHaveLength(1);
    expect(st.unreadNotices).toBe(1);
    expect(st.justCompleted.a).toBeGreaterThan(0);
  });

  it("removes records", () => {
    const s = useStore.getState();
    s.applyEvent({ type: "upserted", record: record({}) });
    s.select("a");
    s.applyEvent({ type: "removed", ids: ["a"] });
    expect(useStore.getState().downloads.a).toBeUndefined();
    expect(useStore.getState().selectedId).toBeNull();
  });

  it("groups statuses into views", () => {
    expect(matchesView(record({ status: "paused" }), "downloading")).toBe(true);
    expect(matchesView(record({ status: "cancelled" }), "failed")).toBe(true);
    expect(matchesView(record({ status: "completed" }), "queued")).toBe(false);
  });
});
