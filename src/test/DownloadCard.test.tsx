import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { TooltipProvider } from "@/components/ui/overlay";
import { DownloadCard } from "@/features/downloads/DownloadCard";
import { useStore } from "@/lib/store";
import type { DownloadRecord } from "@/types/engine";

const base: DownloadRecord = {
  id: "d1",
  url: "https://cdn.example.com/big.iso",
  finalUrl: null,
  fileName: "big.iso",
  nameAuto: false,
  destDir: "C:\\Downloads",
  destAuto: true,
  category: "archive",
  status: "downloading",
  priority: "normal",
  queuePos: 0,
  totalSize: 1000,
  downloaded: 250,
  resumable: true,
  connections: 8,
  speedLimit: null,
  etag: null,
  lastModified: null,
  mime: null,
  referrer: null,
  error: null,
  createdAt: Date.now(),
  startedAt: null,
  completedAt: null,
  scheduledAt: null,
  checksum: null,
  checksumState: "none",
  overwrite: false,
  restartPending: false,
};

function renderCard(record: DownloadRecord, progress?: { downloaded: number; speed: number }) {
  useStore.setState({
    downloads: { [record.id]: record },
    progress: progress ? { [record.id]: { id: record.id, totalSize: record.totalSize, etaSecs: 30, connections: 4, ...progress } } : {},
  });
  return render(
    <TooltipProvider>
      <DownloadCard id={record.id} />
    </TooltipProvider>,
  );
}

describe("DownloadCard", () => {
  beforeEach(() => useStore.setState({ downloads: {}, progress: {}, justCompleted: {} }));

  it("shows live engine numbers for a running download", () => {
    renderCard(base, { downloaded: 500, speed: 2048 });
    expect(screen.getByText("big.iso")).toBeInTheDocument();
    expect(screen.getByText("2.0 KB/s")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "50");
    expect(screen.getByRole("button", { name: "Pause" })).toBeInTheDocument();
  });

  it("offers Retry and shows the reason for a failed download", () => {
    renderCard({ ...base, status: "failed", error: { code: "not_found", message: "The file was not found on the server (HTTP 404)." } });
    expect(screen.getByText(/HTTP 404/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Pause" })).not.toBeInTheDocument();
  });

  it("offers Open and Show in folder only once completed", () => {
    renderCard({ ...base, status: "completed", downloaded: 1000, completedAt: Date.now() });
    expect(screen.getByRole("button", { name: "Open file" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Show in folder" })).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  });
});
