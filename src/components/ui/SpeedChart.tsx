import { useId, useMemo } from "react";
import { motion } from "motion/react";

import { formatSpeed } from "@/lib/format";

/** Smooth path through points using a monotone cubic curve (no overshoot below zero). */
function smoothPath(points: [number, number][]): string {
  if (points.length < 2) return "";
  const n = points.length;
  const dx: number[] = [];
  const m: number[] = [];
  for (let i = 0; i < n - 1; i++) {
    dx.push(points[i + 1][0] - points[i][0]);
    m.push((points[i + 1][1] - points[i][1]) / (dx[i] || 1));
  }
  const t: number[] = [m[0]];
  for (let i = 1; i < n - 1; i++) t.push(m[i - 1] * m[i] <= 0 ? 0 : (m[i - 1] + m[i]) / 2);
  t.push(m[n - 2]);
  let d = `M${points[0][0].toFixed(2)},${points[0][1].toFixed(2)}`;
  for (let i = 0; i < n - 1; i++) {
    const [x0, y0] = points[i];
    const [x1, y1] = points[i + 1];
    const h = dx[i] / 3;
    d += ` C${(x0 + h).toFixed(2)},${(y0 + t[i] * h).toFixed(2)} ${(x1 - h).toFixed(2)},${(y1 - t[i + 1] * h).toFixed(2)} ${x1.toFixed(2)},${y1.toFixed(2)}`;
  }
  return d;
}

/**
 * Real-time throughput chart. `samples` are bytes/s, one per second, oldest first. The series is
 * padded to a fixed width so the line scrolls in from the right rather than stretching.
 */
export function SpeedChart({ samples, height = 160, slots = 90, showAxis = true }: { samples: number[]; height?: number; slots?: number; showAxis?: boolean }) {
  const gid = useId();
  const width = 600;
  const { line, area, max } = useMemo(() => {
    const data = samples.slice(-slots);
    const padded = [...Array(Math.max(0, slots - data.length)).fill(0), ...data];
    const peak = Math.max(...padded, 1);
    // Round the scale up to a "nice" number so the axis doesn't jitter every second.
    const mag = 10 ** Math.floor(Math.log10(peak));
    const nice = Math.ceil(peak / mag) * mag;
    const top = 8;
    const pts: [number, number][] = padded.map((v, i) => [(i / (slots - 1)) * width, top + (1 - v / nice) * (height - top - 2)]);
    const l = smoothPath(pts);
    return { line: l, area: `${l} L${width},${height} L0,${height} Z`, max: nice };
  }, [samples, slots, height]);

  return (
    <div className="relative">
      <svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" className="block w-full" style={{ height }} aria-label="Download speed over time">
        <defs>
          <linearGradient id={`fill-${gid}`} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="var(--accent)" stopOpacity="0.32" />
            <stop offset="1" stopColor="var(--accent)" stopOpacity="0" />
          </linearGradient>
          <linearGradient id={`stroke-${gid}`} x1="0" y1="0" x2="1" y2="0">
            <stop offset="0" stopColor="var(--accent-2)" />
            <stop offset="1" stopColor="var(--accent)" />
          </linearGradient>
        </defs>
        {[0.25, 0.5, 0.75].map((f) => (
          <line key={f} x1="0" x2={width} y1={height * f} y2={height * f} stroke="var(--line)" strokeDasharray="3 5" vectorEffect="non-scaling-stroke" />
        ))}
        <motion.path d={area} fill={`url(#fill-${gid})`} initial={false} animate={{ d: area }} transition={{ duration: 0.45, ease: "easeOut" }} />
        <motion.path
          d={line}
          fill="none"
          stroke={`url(#stroke-${gid})`}
          strokeWidth="2.2"
          vectorEffect="non-scaling-stroke"
          strokeLinecap="round"
          initial={false}
          animate={{ d: line }}
          transition={{ duration: 0.45, ease: "easeOut" }}
        />
      </svg>
      {showAxis && samples.some((v) => v > 0) && (
        <div className="pointer-events-none absolute top-0 left-1 text-[10.5px] font-medium text-subtle tabular">{formatSpeed(max)}</div>
      )}
    </div>
  );
}

export function Sparkline({ samples, height = 28 }: { samples: number[]; height?: number }) {
  return <SpeedChart samples={samples} height={height} slots={40} showAxis={false} />;
}
