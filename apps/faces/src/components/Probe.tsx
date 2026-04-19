"use client";

import { useEffect, useRef, useState } from "react";
import type { Statement } from "@donto/client";
import { isLiveAt, renderObject, type CubePoint } from "@donto/client/history";
import { fmtDate } from "@/lib/colors";

interface Props {
  rows: Statement[];
  bounds: { vMin: number; vMax: number; tMin: number; tMax: number };
  cursor: CubePoint | null;
  onCursor: (p: CubePoint) => void;
  colorOf: (ctx: string) => string;
}

/**
 * Probe — a 2D plane (valid_time × tx_time). Click to place the cursor.
 * The right pane lists every statement that's live at that exact (valid, tx)
 * coordinate — i.e. donto_match for the chosen point of belief.
 */
export function Probe({ rows, bounds, cursor, onCursor, colorOf }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const planeRef = useRef<SVGSVGElement>(null);
  const [size, setSize] = useState({ w: 360, h: 400 });

  useEffect(() => {
    if (!ref.current) return;
    const ro = new ResizeObserver(() => {
      const r = ref.current!.getBoundingClientRect();
      // The plane is the left half (fixed at 360px in CSS); the panel is
      // wider. We let the SVG fill its grid cell.
      setSize({ w: r.width, h: r.height });
    });
    ro.observe(ref.current);
    return () => ro.disconnect();
  }, []);

  const PAD_L = 48, PAD_R = 8, PAD_T = 18, PAD_B = 24;
  const innerW = Math.max(1, size.w - PAD_L - PAD_R);
  const innerH = Math.max(1, size.h - PAD_T - PAD_B);
  const xScale = (v: number) =>
    PAD_L + ((v - bounds.vMin) / (bounds.vMax - bounds.vMin || 1)) * innerW;
  const yScale = (t: number) =>
    PAD_T + ((t - bounds.tMin) / (bounds.tMax - bounds.tMin || 1)) * innerH;
  const xInv = (px: number) =>
    bounds.vMin + ((px - PAD_L) / innerW) * (bounds.vMax - bounds.vMin);
  const yInv = (py: number) =>
    bounds.tMin + ((py - PAD_T) / innerH) * (bounds.tMax - bounds.tMin);

  const matching = cursor
    ? rows.filter((r) => isLiveAt(r, cursor))
    : [];

  return (
    <div className="grid grid-cols-[360px_1fr] h-full">
      <div ref={ref} className="bg-paper border-r border-rule">
        <svg
          ref={planeRef}
          className="probe-svg"
          width={size.w}
          height={size.h}
          onClick={(e) => {
            const rect = planeRef.current!.getBoundingClientRect();
            onCursor({
              valid: xInv(e.clientX - rect.left),
              tx:    yInv(e.clientY - rect.top),
            });
          }}
        >
          {/* subtle grid */}
          {Array.from({ length: 9 }, (_, i) => {
            const xp = PAD_L + (innerW * i) / 8;
            const yp = PAD_T + (innerH * i) / 8;
            return (
              <g key={i}>
                <line x1={xp} y1={PAD_T} x2={xp} y2={size.h - PAD_B} stroke="#161412" />
                <line x1={PAD_L} y1={yp} x2={size.w - PAD_R} y2={yp} stroke="#161412" />
              </g>
            );
          })}
          <text x={6} y={PAD_T + 6} fill="#6a655e" fontSize={9}>tx_time ↓</text>
          <text x={size.w - PAD_R - 60} y={size.h - PAD_B + 14}
            fill="#6a655e" fontSize={9}>valid_time →</text>
          {/* dots: where each statement starts */}
          {rows.map((r) => {
            const vlo = r.valid_lo ? Date.parse(r.valid_lo) : bounds.vMin;
            const tlo = r.tx_lo ? Date.parse(r.tx_lo) : Date.now();
            return (
              <circle key={r.statement_id} cx={xScale(vlo)} cy={yScale(tlo)} r={2}
                fill={colorOf(r.context)} opacity={0.7} />
            );
          })}
          {/* cursor */}
          {cursor && (() => {
            const x = xScale(cursor.valid);
            const y = yScale(cursor.tx);
            return (
              <g>
                <line x1={x} y1={PAD_T} x2={x} y2={size.h - PAD_B}
                  stroke="#f0c674" strokeWidth={1} strokeDasharray="2,2" />
                <line x1={PAD_L} y1={y} x2={size.w - PAD_R} y2={y}
                  stroke="#f0c674" strokeWidth={1} strokeDasharray="2,2" />
                <circle cx={x} cy={y} r={4} fill="#f0c674" />
                <text x={x + 8} y={y - 8} fill="#f0c674" fontSize={10}>
                  valid {fmtDate(cursor.valid)} · believed {fmtDate(cursor.tx)}
                </text>
              </g>
            );
          })()}
        </svg>
      </div>
      <div className="overflow-auto px-3.5 py-2.5 text-xs">
        {matching.length === 0 ? (
          <div className="text-muted text-center py-8">
            {cursor ? "no statements live at this point in the cube"
                    : "click the plane to place the cursor"}
          </div>
        ) : (
          matching.map((r) => (
            <div
              key={r.statement_id}
              className="border-l-2 px-2.5 py-1.5 my-1.5 bg-[#181612]"
              style={{ borderLeftColor: colorOf(r.context) }}
            >
              <div>
                <span className="text-accent">{r.predicate}</span>{" "}
                <span className="text-ink/90">{renderObject(r)}</span>
              </div>
              <div className="text-[#8a847b] text-[11px]">
                {r.context} · polarity: {r.polarity}
                {r.tx_hi ? " · was once believed" : ""}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
