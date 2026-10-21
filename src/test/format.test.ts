import { describe, expect, it } from "vitest";

import { formatBytes, formatEta, formatSpeed, hostOf, parseRate, percent } from "@/lib/format";
import { extractUrls } from "@/features/add/AddDownloadDialog";

describe("formatting", () => {
  it("formats bytes with sensible precision", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(734 * 1024 * 1024)).toBe("734 MB");
    expect(formatBytes(null)).toBe("—");
  });

  it("formats speed and ETA", () => {
    expect(formatSpeed(0)).toBe("0 B/s");
    expect(formatSpeed(2 * 1024 * 1024)).toBe("2.0 MB/s");
    expect(formatEta(42)).toBe("42s");
    expect(formatEta(125)).toBe("2m 05s");
    expect(formatEta(3 * 3600 + 120)).toBe("3h 2m");
    expect(formatEta(null)).toBe("—");
  });

  it("computes percent only when the size is known", () => {
    expect(percent(50, 200)).toBe(25);
    expect(percent(50, null)).toBeNull();
    expect(percent(500, 200)).toBe(100);
  });

  it("parses rate input", () => {
    expect(parseRate("")).toBe(0);
    expect(parseRate("500 KB")).toBe(500 * 1024);
    expect(parseRate("2mb/s")).toBe(2 * 1024 * 1024);
    expect(parseRate("1.5 M")).toBe(Math.round(1.5 * 1024 * 1024));
    expect(parseRate("fast")).toBeNull();
  });

  it("extracts host names", () => {
    expect(hostOf("https://www.example.com/a.zip")).toBe("example.com");
  });
});

describe("link extraction", () => {
  it("finds http(s) links in pasted text and de-duplicates them", () => {
    const text = `Grab these:\nhttps://a.example.com/x.iso, (https://b.example.com/y.zip)\nhttps://a.example.com/x.iso\nftp://nope/z`;
    expect(extractUrls(text)).toEqual(["https://a.example.com/x.iso", "https://b.example.com/y.zip"]);
  });

  it("ignores non-links", () => {
    expect(extractUrls("hello world")).toEqual([]);
  });
});
