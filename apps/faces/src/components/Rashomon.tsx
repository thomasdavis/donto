"use client";

import { useEffect, useRef, useState } from "react";
import type { Statement } from "@donto/client";
import { renderObject } from "@donto/client/history";
import { fmtDate } from "@/lib/colors";
import { Tooltip } from "./Stratigraph";

interface Props {
  rows: Statement[];
  bounds: { vMin: number; vMax: number; tMin: number; tMax: number };
  contexts: string[];
  colorOf: (ctx: string) => string;
}

/**
 * Rashomon Hall — every context gets a horizontal lane. Same valid_time
 * across two lanes with different objects → red connecting line. Eye finds
 * the disagreements automatically.
 */
export function Rashomon({ rows, bounds, contexts, colorOf }: Props) {
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

  const PAD_L = 220, PAD_R = 12, PAD_T = 16, PAD_B = 28;
  const innerW = Math.max(1, size.w - PAD_L - PAD_R);
  const laneH = Math.max(28, (size.h - PAD_T - PAD_B) / Math.max(contexts.length, 1));
  const xScale = (v: number) =>
    PAD_L + ((v - bounds.vMin) / (bounds.vMax - bounds.vMin || 1)) * innerW;

  // Disagreement detection.
  type Pos = { ctx: string; pred: string; obj: string; vlo: number; x: number; y: number };
  const positions: Pos[] = [];
  for (const r of rows) {
    const i = contexts.indexOf(r.context);
    if (i < 0) continue;
    const y = PAD_T + i * laneH + laneH / 2;
    const vlo = r.valid_lo ? Date.parse(r.valid_lo) : bounds.vMin;
    positions.push({ ctx: r.context, pred: r.predicate,
      obj: renderObject(r), vlo, x: xScale(vlo), y });
  }
  const conflicts: [Pos, Pos][] = [];
  const groups = new Map<string, Pos[]>();
  for (const p of positions) {
    const key = `${p.pred}|${p.vlo}`;
    const arr = groups.get(key);
    if (arr) arr.push(p); else groups.set(key, [p]);
  }
  for (const ps of groups.values()) {
    if (ps.length < 2) continue;
    const objs = new Set(ps.map((p) => p.obj));
    if (objs.size < 2) continue;
    for (let i = 0; i < ps.length; i++)
      for (let j = i + 1; j < ps.length; j++)
        conflicts.push([ps[i]!, ps[j]!]);
  }

  return (
    <div ref={ref} className="w-full h-full relative">
      <svg width={size.w} height={size.h}>
        {/* lanes */}
        {contexts.map((ctx, i) => {
          const y = PAD_T + i * laneH;
          return (
            <g key={ctx}>
              <rect x={PAD_L} y={y} width={innerW} height={laneH}
                fill={i % 2 ? "#0e0d0c" : "#131210"} />
              <line x1={PAD_L} y1={y} x2={size.w - PAD_R} y2={y} stroke="#1f1d1a" />
              <text x={8} y={y + laneH / 2 + 4} fill={colorOf(ctx)} fontSize={11}>
                {ctx}
              </text>
            </g>
          );
        })}
        {/* x-axis ticks */}
        {Array.from({ length: 5 }, (_, i) => {
          const xp = PAD_L + (innerW * i) / 4;
          const v = bounds.vMin + ((bounds.vMax - bounds.vMin) * i) / 4;
          return (
            <text key={i} x={xp - 24} y={size.h - PAD_B + 14}
              fill="#6a655e" fontSize={10}>{fmtDate(v)}</text>
          );
        })}
        {/* disagreement lines (drawn underneath dots so dots win) */}
        {conflicts.map(([a, b], i) => (
          <line key={i} x1={a.x} y1={a.y} x2={b.x} y2={b.y}
            stroke="#b0584c" strokeWidth={1.5} opacity={0.7} />
        ))}
        {/* statements */}
        {rows.map((r) => {
          const i = contexts.indexOf(r.context);
          if (i < 0) return null;
          const y = PAD_T + i * laneH + laneH / 2;
          const vlo = r.valid_lo ? Date.parse(r.valid_lo) : bounds.vMin;
          const tip =
            `${r.context}\n${r.predicate} → ${renderObject(r)}\nvalid ${fmtDate(vlo)}\n` +
            `believed ${fmtDate(Date.parse(r.tx_lo))}` +
            `..${r.tx_hi ? fmtDate(Date.parse(r.tx_hi)) : "present"}` +
            (r.tx_hi ? "\n[retracted]" : "");
          return (
            <circle
              key={r.statement_id}
              cx={xScale(vlo)} cy={y} r={6}
              fill={colorOf(r.context)}
              opacity={r.tx_hi ? 0.35 : 1}
              stroke={r.tx_hi ? "#b0584c" : "#0e0d0c"} strokeWidth={1}
              onMouseMove={(e) => setHover({ x: e.clientX, y: e.clientY, text: tip })}
              onMouseLeave={() => setHover(null)}
            />
          );
        })}
      </svg>
      <Tooltip hover={hover} />
    </div>
  );
}
