"use client";

import { useEffect, useRef, useState } from "react";
import type { Statement } from "@donto/client";
import { renderObject } from "@donto/client/history";
import { fmtDate } from "@/lib/colors";

interface Props {
  rows: Statement[];
  bounds: { vMin: number; vMax: number; tMin: number; tMax: number };
  colorOf: (ctx: string) => string;
}

/**
 * Stratigraph — a geological cross-section of belief.
 *   x-axis: valid_time (when the world said it held)
 *   y-axis: tx_time (when donto believed it; older belief = TOP, newer = bottom)
 *
 * Each statement is a rectangle covering its (valid, tx) interval. Retracted
 * statements (closed tx_time) appear with a red border and lower opacity —
 * the band that ended.
 */
export function Stratigraph({ rows, bounds, colorOf }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 800, h: 400 });
  useEffect(() => {
    if (!ref.current) return;
    const ro = new ResizeObserver(() => {
      const r = ref.current!.getBoundingClientRect();
      setSize({ w: r.width, h: r.height });
    });
    ro.observe(ref.current);
    return () => ro.disconnect();
  }, []);

  const PAD_L = 64, PAD_R = 12, PAD_T = 24, PAD_B = 28;
  const innerW = Math.max(1, size.w - PAD_L - PAD_R);
  const innerH = Math.max(1, size.h - PAD_T - PAD_B);
  const xScale = (v: number) =>
    PAD_L + ((v - bounds.vMin) / (bounds.vMax - bounds.vMin || 1)) * innerW;
  const yScale = (t: number) =>
    PAD_T + ((t - bounds.tMin) / (bounds.tMax - bounds.tMin || 1)) * innerH;
  const NOW = Date.now();
  const [hover, setHover] = useState<{ x: number; y: number; text: string } | null>(null);

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
        {/* strata */}
        {rows.map((r) => {
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
        })}
        <text x={PAD_L} y={14} fill="#6a655e" fontSize={10}>↑ believed earlier</text>
        <text x={size.w - PAD_R - 100} y={size.h - PAD_B + 26} fill="#6a655e" fontSize={10}>
          world-time →
        </text>
      </svg>
      <Tooltip hover={hover} />
    </div>
  );
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
