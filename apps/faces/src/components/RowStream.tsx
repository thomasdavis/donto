"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import type { Statement } from "@donto/client";
import { renderObject } from "@donto/client/history";
import { fmtDate } from "@/lib/colors";

/**
 * Virtualised row stream — terminal-style, no resolution.
 *
 * Only the rows in the current scroll viewport are mounted in the DOM.
 * Tested with 20k-row history; idle DOM stays under ~150 nodes.
 */
const ROW_H   = 76;   // matches card height in px
const OVERSCAN = 8;

export function RowStream({
  rows, colorOf,
}: { rows: Statement[]; colorOf: (ctx: string) => string }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [start, setStart] = useState(0);
  const [containerH, setContainerH] = useState(400);

  useEffect(() => {
    const el = containerRef.current; if (!el) return;
    const ro = new ResizeObserver(() => setContainerH(el.clientHeight));
    ro.observe(el);
    setContainerH(el.clientHeight);
    return () => ro.disconnect();
  }, []);

  // Reset to top whenever the row set changes (different subject / filter).
  useEffect(() => {
    if (containerRef.current) containerRef.current.scrollTop = 0;
    setStart(0);
  }, [rows]);

  function onScroll(e: React.UIEvent<HTMLDivElement>) {
    const top = (e.target as HTMLDivElement).scrollTop;
    setStart(Math.max(0, Math.floor(top / ROW_H) - OVERSCAN));
  }

  const end = Math.min(rows.length, start + Math.ceil(containerH / ROW_H) + OVERSCAN * 2);
  const visible = useMemo(() => rows.slice(start, end), [rows, start, end]);
  const totalH  = rows.length * ROW_H;

  return (
    <div ref={containerRef} onScroll={onScroll}
         className="px-4 py-3 text-xs overflow-auto h-full">
      <div style={{ height: totalH, position: "relative" }}>
        <div style={{ position: "absolute", top: start * ROW_H, left: 0, right: 0 }}>
          {visible.map((r) => {
            const vlo = r.valid_lo ? Date.parse(r.valid_lo) : null;
            const vhi = r.valid_hi ? Date.parse(r.valid_hi) : null;
            const tlo = Date.parse(r.tx_lo);
            const thi = r.tx_hi ? Date.parse(r.tx_hi) : null;
            return (
              <div
                key={r.statement_id}
                className="border-l-2 px-3 py-1.5 my-2 bg-[#181612]"
                style={{
                  height: ROW_H - 8,
                  borderLeftColor: r.tx_hi ? "#b0584c"
                    : (r.lineage.length ? "#82a45a" : colorOf(r.context)),
                  opacity: r.tx_hi ? 0.6 : 1,
                  overflow: "hidden",
                }}
              >
                <div>
                  <span className="text-accent font-medium">{r.context}</span>{"  "}
                  <span>{r.predicate}</span>
                </div>
                <div className="text-ink/90 truncate">&quot;{renderObject(r)}&quot;</div>
                <div className="text-muted text-[11px]">
                  valid {fmtDate(vlo)}{vhi ? `..${fmtDate(vhi)}` : ""}
                  {" · "}
                  believed {fmtDate(tlo)}
                  {thi ? ` → ${fmtDate(thi)} (retracted)` : " → present"}
                  {r.lineage.length > 0 && <> · lineage: {r.lineage.length}</>}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
