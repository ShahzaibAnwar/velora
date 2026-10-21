// Inspect the icon resources embedded in a Windows executable, or validate a standalone .ico.
//
//   node scripts/inspect-pe-icons.mjs <file.exe|file.ico> [--extract out.ico]
//
// For an EXE it walks the PE resource directory, lists RT_GROUP_ICON (14) and RT_ICON (3) entries,
// checks that every group entry points at an existing RT_ICON with the declared size, validates each
// image (PNG header or BITMAPINFOHEADER with consistent dimensions and data length), and can rebuild
// the group as a standalone .ico for visual comparison. Exit code 1 on any structural problem.

import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";

const [file, flag, extractPath] = process.argv.slice(2);
if (!file) {
  console.error("usage: node scripts/inspect-pe-icons.mjs <file.exe|file.ico> [--extract out.ico]");
  process.exit(2);
}
const buf = readFileSync(file);
const problems = [];
console.log(`${file}\n  size ${buf.length} bytes, sha256 ${createHash("sha256").update(buf).digest("hex")}`);

function describeImage(data, label) {
  if (data.length >= 24 && data.readUInt32BE(0) === 0x89504e47) {
    const w = data.readUInt32BE(16);
    const h = data.readUInt32BE(20);
    const ok = data.subarray(12, 16).toString("latin1") === "IHDR";
    if (!ok) problems.push(`${label}: PNG without IHDR`);
    return { kind: "PNG", w, h, bpp: data[24] * (data[25] === 6 ? 4 : data[25] === 2 ? 3 : 1), ok };
  }
  if (data.length < 40 || data.readUInt32LE(0) !== 40) {
    problems.push(`${label}: neither PNG nor BITMAPINFOHEADER`);
    return { kind: "INVALID", w: 0, h: 0, bpp: 0, ok: false };
  }
  const w = data.readInt32LE(4);
  const h2 = data.readInt32LE(8);
  const bpp = data.readUInt16LE(14);
  const colors = data.readUInt32LE(32) || (bpp <= 8 ? 1 << bpp : 0);
  const xor = Math.ceil((w * bpp) / 32) * 4 * w;
  const and = Math.ceil(w / 32) * 4 * w;
  const expected = 40 + colors * 4 + xor + and;
  const ok = h2 === 2 * w && data.length >= expected;
  if (!ok) problems.push(`${label}: BMP ${w}x${h2 / 2} ${bpp}bpp has ${data.length} bytes, expected >= ${expected} (height field ${h2})`);
  return { kind: "BMP", w, h: h2 / 2, bpp, ok };
}

function listIco(ico, label) {
  const count = ico.readUInt16LE(4);
  if (ico.readUInt16LE(0) !== 0 || ico.readUInt16LE(2) !== 1) problems.push(`${label}: bad ICO header`);
  const rows = [];
  for (let i = 0; i < count; i++) {
    const o = 6 + 16 * i;
    const dw = ico[o] || 256;
    const len = ico.readUInt32LE(o + 8);
    const off = ico.readUInt32LE(o + 12);
    const img = describeImage(ico.subarray(off, off + len), `${label} entry ${i}`);
    if (img.w !== dw) problems.push(`${label} entry ${i}: directory says ${dw}px, image is ${img.w}px`);
    rows.push(`    ${String(dw).padStart(3)}px  ${img.kind}  ${img.w}x${img.h} ${img.bpp}bpp  ${len} bytes  ${img.ok ? "valid" : "INVALID"}`);
  }
  console.log(`  ${count} images:\n${rows.join("\n")}`);
}

if (buf.readUInt16LE(0) === 0 && buf.readUInt16LE(2) === 1) {
  listIco(buf, "ico");
} else {
  // --- PE parsing ---------------------------------------------------------------------------------
  if (buf.toString("latin1", 0, 2) !== "MZ") throw new Error("not a PE file");
  const pe = buf.readUInt32LE(0x3c);
  if (buf.readUInt32LE(pe) !== 0x4550) throw new Error("missing PE signature");
  const sections = buf.readUInt16LE(pe + 6);
  const optSize = buf.readUInt16LE(pe + 20);
  const opt = pe + 24;
  const magic = buf.readUInt16LE(opt);
  const ddBase = opt + (magic === 0x20b ? 112 : 96);
  const rsrcRva = buf.readUInt32LE(ddBase + 2 * 8);
  const secTable = opt + optSize;
  const secs = [];
  for (let i = 0; i < sections; i++) {
    const s = secTable + 40 * i;
    secs.push({ name: buf.toString("latin1", s, s + 8).replace(/\0+$/, ""), va: buf.readUInt32LE(s + 12), vsize: buf.readUInt32LE(s + 8), raw: buf.readUInt32LE(s + 20), rawSize: buf.readUInt32LE(s + 16) });
  }
  const rvaToOff = (rva) => {
    const s = secs.find((x) => rva >= x.va && rva < x.va + Math.max(x.vsize, x.rawSize));
    if (!s) throw new Error(`RVA 0x${rva.toString(16)} outside sections`);
    return rva - s.va + s.raw;
  };
  console.log(`  PE${magic === 0x20b ? "32+" : "32"}, subsystem ${buf.readUInt16LE(opt + 68)} (2 = GUI), sections: ${secs.map((s) => s.name).join(" ")}`);
  if (!rsrcRva) {
    problems.push("no resource directory");
  } else {
    const base = rvaToOff(rsrcRva);
    const entries = (dirOff) => {
      const named = buf.readUInt16LE(dirOff + 12);
      const ids = buf.readUInt16LE(dirOff + 14);
      const out = [];
      for (let i = 0; i < named + ids; i++) {
        const e = dirOff + 16 + 8 * i;
        const nameField = buf.readUInt32LE(e);
        const dataField = buf.readUInt32LE(e + 4);
        let name;
        if (nameField & 0x80000000) {
          const so = base + (nameField & 0x7fffffff);
          name = buf.toString("utf16le", so + 2, so + 2 + 2 * buf.readUInt16LE(so));
        } else name = nameField;
        out.push({ name, sub: (dataField & 0x80000000) !== 0, off: base + (dataField & 0x7fffffff) });
      }
      return out;
    };
    const leaf = (e) => {
      // type -> name -> language -> data entry
      let cur = e;
      while (cur.sub) cur = entries(cur.off)[0];
      const rva = buf.readUInt32LE(cur.off);
      const size = buf.readUInt32LE(cur.off + 4);
      return buf.subarray(rvaToOff(rva), rvaToOff(rva) + size);
    };
    const types = entries(base);
    const typeOf = (id) => types.find((t) => t.name === id);
    const iconType = typeOf(3);
    const groupType = typeOf(14);
    const icons = new Map((iconType ? entries(iconType.off) : []).map((e) => [e.name, leaf(e)]));
    const groups = groupType ? entries(groupType.off) : [];
    console.log(`  RT_ICON resources: ${icons.size}; RT_GROUP_ICON resources: ${groups.length} (${groups.map((g) => (typeof g.name === "number" ? `#${g.name}` : g.name)).join(", ")})`);
    if (!groups.length) problems.push("no RT_GROUP_ICON — Explorer would show a generic icon");
    for (const g of groups) {
      const grp = leaf(g);
      const count = grp.readUInt16LE(4);
      if (grp.readUInt16LE(2) !== 1) problems.push(`group ${g.name}: type is not icon`);
      console.log(`  group #${g.name}: ${count} entries`);
      const parts = [];
      for (let i = 0; i < count; i++) {
        const o = 6 + 14 * i;
        const w = grp[o] || 256;
        const bpp = grp.readUInt16LE(o + 6);
        const bytes = grp.readUInt32LE(o + 8);
        const id = grp.readUInt16LE(o + 12);
        const data = icons.get(id);
        if (!data) {
          problems.push(`group ${g.name} entry ${i}: RT_ICON #${id} missing`);
          console.log(`    ${String(w).padStart(3)}px -> RT_ICON #${id}: MISSING`);
          continue;
        }
        if (data.length !== bytes) problems.push(`group ${g.name} entry ${i}: declares ${bytes} bytes, RT_ICON #${id} has ${data.length}`);
        const img = describeImage(data, `RT_ICON #${id}`);
        if (img.w !== w) problems.push(`RT_ICON #${id}: group says ${w}px, image is ${img.w}px`);
        console.log(`    ${String(w).padStart(3)}px ${bpp}bpp -> RT_ICON #${id}: ${img.kind} ${img.w}x${img.h}, ${data.length} bytes, ${img.ok && data.length === bytes ? "valid" : "INVALID"}`);
        parts.push({ entry: grp.subarray(o, o + 12), data });
      }
      if (flag === "--extract" && extractPath && g === groups[0]) {
        const header = Buffer.alloc(6 + 16 * parts.length);
        header.writeUInt16LE(1, 2);
        header.writeUInt16LE(parts.length, 4);
        let off = header.length;
        parts.forEach((p, i) => {
          p.entry.copy(header, 6 + 16 * i, 0, 8);
          header.writeUInt32LE(p.data.length, 6 + 16 * i + 8);
          header.writeUInt32LE(off, 6 + 16 * i + 12);
          off += p.data.length;
        });
        writeFileSync(extractPath, Buffer.concat([header, ...parts.map((p) => p.data)]));
        console.log(`  extracted group #${g.name} to ${extractPath}`);
      }
    }
  }
}

if (problems.length) {
  console.log(`  PROBLEMS:\n${problems.map((p) => "    - " + p).join("\n")}`);
  process.exit(1);
}
console.log("  no structural problems found");
