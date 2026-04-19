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

const DOT_THRESHOLD = 1500;   // switch to binned dots above this row count

/**
 * Rashomon Hall — every context gets a horizontal lane. Same predicate +
 * valid_time across two lanes with different objects → red connector.
 *
 * Scaling:
 *   * Disagreement detection is O(N) — keyed by (predicate, vlo).
 *   * Above DOT_THRESHOLD, each lane bins by valid_time; one circle per
 *     bin, radius scaled by the bin's row count. Disagreement lines still
 *     drawn between bins that contain conflicting predicates.
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
  const laneH  = Math.max(28, (size.h - PAD_T - PAD_B) / Math.max(contexts.length, 1));
  const xScale = (v: number) =>
    PAD_L + ((v - bounds.vMin) / (bounds.vMax - bounds.vMin || 1)) * innerW;

  const dense = rows.length > DOT_THRESHOLD;

  // ── Detail-mode positions (one circle per row) ─────────────────────────
  type Pos = { ctx: string; pred: string; obj: string; vlo: number; x: number; y: number; row: Statement };
  const positions: Pos[] = [];
  if (!dense) {
    for (const r of rows) {
      const i = contexts.indexOf(r.context);
      if (i < 0) continue;
      const y = PAD_T + i * laneH + laneH / 2;
      const vlo = r.valid_lo ? Date.parse(r.valid_lo) : bounds.vMin;
      positions.push({
        ctx: r.context, pred: r.predicate, obj: renderObject(r),
        vlo, x: xScale(vlo), y, row: r,
      });
    }
  }

  // ── Disagreement detection: O(N) via Map keyed on (predicate|vlo) ──────
  // We use this in BOTH detail and dense modes — for dense mode we just
  // map the conflict bin onto the bin centre.
  const conflicts: Array<[Pos, Pos]> = [];
  if (!dense) {
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
  }

  // ── Dense-mode bins ────────────────────────────────────────────────────
  const NX = 80;
  const cellW = innerW / NX;
  type Bin = { ix: number; ctx: string; count: number; predicates: Set<string>; x: number; y: number };
  const bins: Bin[] = [];
  if (dense) {
    const grid = new Map<string, Bin>(); // key: `${ctx}|${ix}`
    for (const r of rows) {
      const i = contexts.indexOf(r.context);
      if (i < 0) continue;
      const y = PAD_T + i * laneH + laneH / 2;
      const vlo = r.valid_lo ? Date.parse(r.valid_lo) : bounds.vMin;
      const ix = Math.min(NX - 1, Math.max(0,
        Math.floor(((vlo - bounds.vMin) / (bounds.vMax - bounds.vMin || 1)) * NX)));
      const key = `${r.context}|${ix}`;
      let b = grid.get(key);
      if (!b) {
        b = { ix, ctx: r.context, count: 0, predicates: new Set(),
              x: PAD_L + (ix + 0.5) * cellW, y };
        grid.set(key, b);
      }
      b.count++;
      b.predicates.add(r.predicate);
    }
    bins.push(...grid.values());

    // Dense-mode disagreement: same valid bucket, predicate appearing in
    // ≥ 2 contexts. We draw a line between the bin centres.
    const byPredBin = new Map<string, Bin[]>();
    for (const b of bins) for (const pred of b.predicates) {
      const k = `${pred}|${b.ix}`;
      const arr = byPredBin.get(k);
      if (arr) arr.push(b); else byPredBin.set(k, [b]);
    }
    for (const arr of byPredBin.values()) {
      if (arr.length < 2) continue;
      const distinct = new Set(arr.map((x) => x.ctx));
      if (distinct.size < 2) continue;
      for (let i = 0; i < arr.length; i++)
        for (let j = i + 1; j < arr.length; j++) {
          const a = arr[i]!, c = arr[j]!;
          if (a.ctx === c.ctx) continue;
          conflicts.push([
            { ctx: a.ctx, pred: "(bin)", obj: "", vlo: 0, x: a.x, y: a.y, row: rows[0]! },
            { ctx: c.ctx, pred: "(bin)", obj: "", vlo: 0, x: c.x, y: c.y, row: rows[0]! },
          ]);
        }
    }
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
                {ctx.length > 32 ? ctx.slice(0, 30) + "…" : ctx}
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
        {/* disagreement lines (under dots) */}
        {conflicts.map(([a, b], i) => (
          <line key={i} x1={a.x} y1={a.y} x2={b.x} y2={b.y}
            stroke="#b0584c" strokeWidth={1.5} opacity={0.65} />
        ))}
        {/* statements (or bins) */}
        {dense
          ? bins.map((b) => {
              const r = Math.min(laneH * 0.45, 3 + Math.sqrt(b.count) * 1.4);
              const tip =
                `${b.ctx}\n${b.count} statement${b.count === 1 ? "" : "s"}\n` +
                `${b.predicates.size} distinct predicate${b.predicates.size === 1 ? "" : "s"}`;
              return (
                <circle key={`${b.ctx}|${b.ix}`}
                  cx={b.x} cy={b.y} r={r}
                  fill={colorOf(b.ctx)} opacity={0.85}
                  stroke="#0e0d0c" strokeWidth={1}
                  onMouseMove={(e) => setHover({ x: e.clientX, y: e.clientY, text: tip })}
                  onMouseLeave={() => setHover(null)}
                />
              );
            })
          : positions.map((p) => {
              const r = p.row;
              const tip =
                `${r.context}\n${r.predicate} → ${renderObject(r)}\nvalid ${fmtDate(p.vlo)}\n` +
                `believed ${fmtDate(Date.parse(r.tx_lo))}` +
                `..${r.tx_hi ? fmtDate(Date.parse(r.tx_hi)) : "present"}` +
                (r.tx_hi ? "\n[retracted]" : "");
              return (
                <circle key={r.statement_id} cx={p.x} cy={p.y} r={6}
                  fill={colorOf(r.context)}
                  opacity={r.tx_hi ? 0.35 : 1}
                  stroke={r.tx_hi ? "#b0584c" : "#0e0d0c"} strokeWidth={1}
                  onMouseMove={(e) => setHover({ x: e.clientX, y: e.clientY, text: tip })}
                  onMouseLeave={() => setHover(null)}
                />
              );
            })
        }
        {dense && (
          <text x={size.w - PAD_R - 220} y={14} fill="#f0c674" fontSize={10}>
            density mode · radius = √(bin row count)
          </text>
        )}
      </svg>
      <Tooltip hover={hover} />
    </div>
  );
}
