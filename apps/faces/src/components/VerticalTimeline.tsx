"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import type { Statement } from "@donto/client";
import { renderObject } from "@donto/client/history";
import { fmtDate, makeColorMap } from "@/lib/colors";

interface Props {
  rows: Statement[];
  subjectIri: string;
  subjectLabel?: string | null;
  /** Default true. False = oldest at top. */
  newestFirst?: boolean;
  onSelect?: (statementId: string | null) => void;
}

const ROW_H        = 84;   // event row height (px)
const YEAR_H       = 56;   // year-divider height (px)
const OVERSCAN     = 6;

/**
 * Date axis = `valid_time` (when the world says it held). NOT `tx_time`
 * (which would just be the ingestion timestamp and would clump 200 years
 * of genealogy into last Tuesday). Returns 0 for undated rows; those are
 * rendered separately under an "(undated)" header instead of being
 * pinned to a fake date.
 */
function rowDateMs(r: Statement): number {
  if (!r.valid_lo) return 0;
  const t = Date.parse(r.valid_lo);
  return Number.isNaN(t) ? 0 : t;
}

/**
 * VerticalTimeline — donto-faces' default explorer view.
 *
 * Time runs top-to-bottom (newest first by default). Each event is a fixed-
 * height row; year boundaries become large divider rows. Virtualised so a
 * 20k-row history stays smooth.
 *
 * Layout per event:
 *   [date col] [colored stripe (context)] [predicate · context] [object]
 *                                          [believed-window meta]
 */
export function VerticalTimeline({
  rows, subjectIri, subjectLabel, newestFirst = true, onSelect,
}: Props) {
  const colorOf = useMemo(() => makeColorMap(), []);
  const containerRef = useRef<HTMLDivElement>(null);
  const [start, setStart] = useState(0);
  const [containerH, setContainerH] = useState(600);

  useEffect(() => {
    const el = containerRef.current; if (!el) return;
    const ro = new ResizeObserver(() => setContainerH(el.clientHeight));
    ro.observe(el);
    setContainerH(el.clientHeight);
    return () => ro.disconnect();
  }, []);

  // Split into dated + undated. Both render. Undated stays visible under
  // its own header — never silently dropped, never pinned to a fake date.
  const { datedRows, undatedRows } = useMemo(() => {
    const dated:   { r: Statement; ms: number }[] = [];
    const undated: Statement[] = [];
    for (const r of rows) {
      const ms = rowDateMs(r);
      if (ms > 0) dated.push({ r, ms });
      else        undated.push(r);
    }
    dated.sort((a, b) => (newestFirst ? b.ms - a.ms : a.ms - b.ms));
    return { datedRows: dated.map((x) => x.r), undatedRows: undated };
  }, [rows, newestFirst]);

  // Build the virtual scroll list. Three item kinds:
  //   * "section" — the "(undated)" header, when there are undated rows
  //   * "year"    — chronological year divider
  //   * "row"     — an event card
  // Each carries its absolute pixel offset so the virtualiser doesn't
  // assume uniform height.
  type Item =
    | { type: "year";    year: number;     top: number; height: number }
    | { type: "section"; label: string;     top: number; height: number }
    | { type: "row";     row: Statement;   top: number; height: number };
  const { items, totalH } = useMemo(() => {
    const out: Item[] = [];
    let cursor = 0;

    // Undated section first (always at the top, regardless of order).
    if (undatedRows.length > 0) {
      out.push({ type: "section",
        label: `${undatedRows.length.toLocaleString()} undated · no valid_time recorded`,
        top: cursor, height: YEAR_H });
      cursor += YEAR_H;
      for (const r of undatedRows) {
        out.push({ type: "row", row: r, top: cursor, height: ROW_H });
        cursor += ROW_H;
      }
    }

    // Then the dated stream, with year dividers.
    let lastYear: number | null = null;
    for (const r of datedRows) {
      const y = new Date(rowDateMs(r)).getUTCFullYear();
      if (y !== lastYear) {
        out.push({ type: "year", year: y, top: cursor, height: YEAR_H });
        cursor += YEAR_H;
        lastYear = y;
      }
      out.push({ type: "row", row: r, top: cursor, height: ROW_H });
      cursor += ROW_H;
    }
    return { items: out, totalH: cursor };
  }, [datedRows, undatedRows]);

  // Window calc — binary-search the start index for the current scroll top.
  function findStartIndex(scrollTop: number): number {
    if (items.length === 0) return 0;
    let lo = 0, hi = items.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      const item = items[mid]!;
      if (item.top + item.height < scrollTop) lo = mid + 1;
      else hi = mid;
    }
    return Math.max(0, lo - OVERSCAN);
  }

  function onScroll(e: React.UIEvent<HTMLDivElement>) {
    setStart(findStartIndex((e.target as HTMLDivElement).scrollTop));
  }

  // Reset to top when the data set changes.
  useEffect(() => {
    if (containerRef.current) containerRef.current.scrollTop = 0;
    setStart(0);
  }, [rows, newestFirst]);

  const end = Math.min(
    items.length,
    start + Math.ceil(containerH / Math.min(ROW_H, YEAR_H)) + OVERSCAN * 2,
  );
  const visible = items.slice(start, end);
  const offset = items[start]?.top ?? 0;

  return (
    <div className="flex flex-col h-full">
      <div className="px-4 py-3 border-b border-rule">
        <div className="text-ink text-base">{subjectLabel ?? "(unlabelled)"}</div>
        <div className="text-muted text-[11px] break-all">
          {subjectIri}
          {" · "}
          {datedRows.length.toLocaleString()} dated event{datedRows.length === 1 ? "" : "s"}
          {undatedRows.length > 0 && (
            <> · {undatedRows.length.toLocaleString()} undated</>
          )}
        </div>
        <div className="text-muted text-[10px] mt-0.5">
          time axis = valid_time (world-time) · {newestFirst ? "↑ newest first" : "↓ oldest first"}
        </div>
      </div>
      <div ref={containerRef} onScroll={onScroll} className="flex-1 overflow-auto">
        <div style={{ height: totalH, position: "relative" }}>
          <div style={{ position: "absolute", top: offset, left: 0, right: 0 }}>
            {visible.map((it) => {
              if (it.type === "year") {
                return <YearHeader key={`y${it.year}-${it.top}`} year={it.year} />;
              }
              if (it.type === "section") {
                return <SectionHeader key={`s-${it.top}`} label={it.label} />;
              }
              return <EventRow
                key={it.row.statement_id}
                row={it.row}
                color={colorOf(it.row.context ?? "")}
                onClick={() => onSelect?.(it.row.statement_id)}
              />;
            })}
          </div>
        </div>
      </div>
    </div>
  );
}

function YearHeader({ year }: { year: number }) {
  return (
    <div
      style={{ height: YEAR_H }}
      className="flex items-end px-6 pb-1.5 border-b border-rule/40"
    >
      <div className="text-accent text-2xl font-light tracking-[0.05em]">{year}</div>
    </div>
  );
}

function SectionHeader({ label }: { label: string }) {
  return (
    <div
      style={{ height: YEAR_H }}
      className="flex items-end px-6 pb-1.5 border-b border-rule/40 bg-paper"
    >
      <div className="text-muted text-[11px] uppercase tracking-[0.16em]">{label}</div>
    </div>
  );
}

function EventRow({
  row, color, onClick,
}: { row: Statement; color: string; onClick?: () => void }) {
  const obj = String(renderObject(row) ?? "");
  // Undated rows render under the "(undated)" section with an em-dash in
  // the date column rather than a fake fallback.
  const dateStr = row.valid_lo ? row.valid_lo.slice(0, 10) : "—";
  const isRetracted = !!row.tx_hi;
  const isDerived   = Array.isArray(row.lineage) && row.lineage.length > 0;
  const ctx  = row.context   ?? "(no context)";
  const pred = row.predicate ?? "(no predicate)";
  const accentColor =
    isRetracted ? "text-retract" :
    isDerived   ? "text-derived" :
                  "text-accent";

  return (
    <div
      onClick={onClick}
      className="flex items-stretch border-b border-rule/30 hover:bg-panel cursor-pointer"
      style={{ height: ROW_H, opacity: isRetracted ? 0.62 : 1 }}
    >
      <div className="w-[110px] shrink-0 px-3 py-2 text-right border-r border-rule/40 font-mono">
        <div className="text-ink text-[12px]">{dateStr}</div>
        {row.valid_hi && (
          <div className="text-muted text-[10px]">→ {row.valid_hi.slice(0, 10)}</div>
        )}
      </div>
      <div className="w-1 shrink-0" style={{ backgroundColor: color }} />
      <div className="flex-1 min-w-0 px-3 py-2">
        <div className="flex items-baseline gap-2">
          <span className={`text-[10px] uppercase tracking-wider ${accentColor}`}>
            {pred}
          </span>
          <span className="text-muted text-[10px] truncate" title={ctx}>
            {ctx.length > 40 ? ctx.slice(0, 38) + "…" : ctx}
          </span>
          {isRetracted && (
            <span className="text-retract text-[9px] uppercase tracking-wider">retracted</span>
          )}
          {isDerived && !isRetracted && (
            <span className="text-derived text-[9px] uppercase tracking-wider">
              derived ({row.lineage.length})
            </span>
          )}
        </div>
        <div
          className="text-ink text-[13px] truncate"
          style={isRetracted ? { textDecoration: "line-through" } : undefined}
        >
          {obj || <span className="text-muted">(no value)</span>}
        </div>
        <div className="text-muted text-[10px]">
          believed {fmtDate(Date.parse(row.tx_lo))}
          {isRetracted ? ` → ${fmtDate(Date.parse(row.tx_hi!))}` : " → present"}
        </div>
      </div>
    </div>
  );
}
