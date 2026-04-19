"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import type { Statement } from "@donto/client";
import { renderObject } from "@donto/client/history";
import { fmtDate } from "@/lib/colors";

interface Props {
  rows: Statement[];
  bounds: { vMin: number; vMax: number; tMin: number; tMax: number };
  colorOf: (ctx: string) => string;
}

const DENSITY_THRESHOLD = 800;   // switch to bin mode above this row count

/**
 * Stratigraph — geological cross-section of belief.
 *   x = valid_time, y = tx_time (down = older belief).
 *
 * ≤ DENSITY_THRESHOLD rows: each statement is its own coloured stratum.
 *  > DENSITY_THRESHOLD rows: bin into a grid; each tile is a heatmap cell
 *    coloured by density. Hovering a tile lists the contexts and counts
 *    that contributed.
 */
export function Stratigraph({ rows, bounds, colorOf }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 800, h: 400 });
  const [hover, setHover] = useState<{ x: number; y: number; text: string } | null>(null);

  useEffect(() => {
    if (!ref.current) return;
    const ro = new ResizeObserver(() => {
      const r = ref.current!.getBoundingClientRect();
      setSize({ w: r.width, h: r.height });
    });
    ro.observe(ref.current);
    return () => ro.disconnect();
  }, []);

  const dense = rows.length > DENSITY_THRESHOLD;
  const PAD_L = 64, PAD_R = 12, PAD_T = 24, PAD_B = 28;
  const innerW = Math.max(1, size.w - PAD_L - PAD_R);
  const innerH = Math.max(1, size.h - PAD_T - PAD_B);
  const xScale = (v: number) =>
    PAD_L + ((v - bounds.vMin) / (bounds.vMax - bounds.vMin || 1)) * innerW;
  const yScale = (t: number) =>
    PAD_T + ((t - bounds.tMin) / (bounds.tMax - bounds.tMin || 1)) * innerH;
  const NOW = Date.now();

  return (
    <div ref={ref} className="w-full h-full relative">
      <svg width={size.w} height={size.h}>
        {/* grid */}
        {Array.from({ length: 5 }, (_, i) => {
          const yp = PAD_T + (innerH * i) / 4;
          const t = bounds.tMin + ((bounds.tMax - bounds.tMin) * i) / 4;
          return (
            <g key={`y${i}`}>
              <line x1={PAD_L} y1={yp} x2={size.w - PAD_R} y2={yp} stroke="#1f1d1a" />
              <text x={6} y={yp + 4} fill="#6a655e" fontSize={10}>{fmtDate(t)}</text>
            </g>
          );
        })}
        {Array.from({ length: 5 }, (_, i) => {
          const xp = PAD_L + (innerW * i) / 4;
          const v = bounds.vMin + ((bounds.vMax - bounds.vMin) * i) / 4;
          return (
            <g key={`x${i}`}>
              <line x1={xp} y1={PAD_T} x2={xp} y2={size.h - PAD_B} stroke="#1f1d1a" />
              <text x={xp - 24} y={size.h - PAD_B + 14} fill="#6a655e" fontSize={10}>
                {fmtDate(v)}
              </text>
            </g>
          );
        })}

        {dense
          ? <DensityBins rows={rows} bounds={bounds} innerW={innerW} innerH={innerH}
                         padL={PAD_L} padT={PAD_T} colorOf={colorOf} setHover={setHover} />
          : rows.map((r) => {
              const vlo = r.valid_lo ? Date.parse(r.valid_lo) : bounds.vMin;
              const vhi = r.valid_hi ? Date.parse(r.valid_hi) : bounds.vMax;
              const tlo = r.tx_lo ? Date.parse(r.tx_lo) : NOW;
              const thi = r.tx_hi ? Date.parse(r.tx_hi) : NOW;
              const x = xScale(vlo);
              const w = Math.max(2, xScale(vhi) - x);
              const y = yScale(tlo);
              const h = Math.max(3, yScale(thi) - y);
              const tip =
                `${r.context}\n${r.predicate} → ${renderObject(r)}\n` +
                `valid ${fmtDate(vlo)}..${r.valid_hi ? fmtDate(vhi) : "∞"}\n` +
                `believed ${fmtDate(tlo)}..${r.tx_hi ? fmtDate(thi) : "present"}` +
                (r.tx_hi ? "\n[retracted]" : "");
              return (
                <rect
                  key={r.statement_id}
                  x={x} y={y} width={w} height={h}
                  fill={colorOf(r.context)}
                  opacity={r.tx_hi ? 0.4 : 0.85}
                  stroke={r.tx_hi ? "#b0584c" : "none"}
                  strokeWidth={r.tx_hi ? 1 : 0}
                  onMouseMove={(e) => setHover({ x: e.clientX, y: e.clientY, text: tip })}
                  onMouseLeave={() => setHover(null)}
                />
              );
            })
        }

        <text x={PAD_L} y={14} fill="#6a655e" fontSize={10}>↑ believed earlier</text>
        <text x={size.w - PAD_R - 100} y={size.h - PAD_B + 26} fill="#6a655e" fontSize={10}>
          world-time →
        </text>
        {dense && (
          <text x={size.w - PAD_R - 200} y={14} fill="#f0c674" fontSize={10}>
            density mode · {rows.length} statements binned
          </text>
        )}
      </svg>
      <Tooltip hover={hover} />
    </div>
  );
}

interface DensityProps {
  rows: Statement[];
  bounds: { vMin: number; vMax: number; tMin: number; tMax: number };
  innerW: number;
  innerH: number;
  padL: number;
  padT: number;
  colorOf: (ctx: string) => string;
  setHover: (h: { x: number; y: number; text: string } | null) => void;
}

/** A grid of (valid_bucket, tx_bucket) tiles, coloured by density. The tile's
 *  fill colour is taken from the most-frequent context in that cell, so the
 *  visual texture still tracks "where each context dominated." */
function DensityBins({ rows, bounds, innerW, innerH, padL, padT, colorOf, setHover }: DensityProps) {
  const NX = 60, NY = 40;
  const cellW = innerW / NX, cellH = innerH / NY;
  type Cell = { count: number; contexts: Map<string, number>; retracted: number };
  const grid: Cell[][] = Array.from({ length: NX }, () =>
    Array.from({ length: NY }, () => ({ count: 0, contexts: new Map(), retracted: 0 })));

  const NOW = Date.now();
  const xRange = bounds.vMax - bounds.vMin || 1;
  const yRange = bounds.tMax - bounds.tMin || 1;
  let maxCount = 0;
  for (const r of rows) {
    const vlo = r.valid_lo ? Date.parse(r.valid_lo) : bounds.vMin;
    const tlo = r.tx_lo ? Date.parse(r.tx_lo) : NOW;
    const ix = Math.min(NX - 1, Math.max(0, Math.floor(((vlo - bounds.vMin) / xRange) * NX)));
    const iy = Math.min(NY - 1, Math.max(0, Math.floor(((tlo - bounds.tMin) / yRange) * NY)));
    const cell = grid[ix]![iy]!;
    cell.count++;
    cell.contexts.set(r.context, (cell.contexts.get(r.context) ?? 0) + 1);
    if (r.tx_hi) cell.retracted++;
    if (cell.count > maxCount) maxCount = cell.count;
  }

  const out: React.ReactElement[] = [];
  for (let ix = 0; ix < NX; ix++) {
    for (let iy = 0; iy < NY; iy++) {
      const cell = grid[ix]![iy]!;
      if (cell.count === 0) continue;
      // Dominant context wins the colour; opacity = density.
      let topCtx = "", topN = 0;
      for (const [c, n] of cell.contexts) if (n > topN) { topCtx = c; topN = n; }
      const opacity = 0.25 + 0.7 * Math.min(1, cell.count / maxCount);
      const tip =
        `${cell.count} statement${cell.count === 1 ? "" : "s"} ` +
        `(${cell.retracted} retracted)\n` +
        [...cell.contexts.entries()]
          .sort((a, b) => b[1] - a[1])
          .slice(0, 5)
          .map(([c, n]) => `  ${c}: ${n}`)
          .join("\n");
      out.push(
        <rect
          key={`${ix},${iy}`}
          x={padL + ix * cellW}
          y={padT + iy * cellH}
          width={cellW + 0.5}
          height={cellH + 0.5}
          fill={colorOf(topCtx)}
          opacity={opacity}
          onMouseMove={(e) => setHover({ x: e.clientX, y: e.clientY, text: tip })}
          onMouseLeave={() => setHover(null)}
        />
      );
    }
  }
  return <>{out}</>;
}

export function Tooltip({ hover }: { hover: { x: number; y: number; text: string } | null }) {
  if (!hover) return null;
  return (
    <div
      className="fixed z-50 pointer-events-none bg-panel border border-rule text-ink
                 text-[11px] leading-snug whitespace-pre-wrap max-w-sm px-2.5 py-1.5"
      style={{ left: hover.x + 12, top: hover.y + 12 }}
    >
      {hover.text}
    </div>
  );
}
