"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import type { Statement } from "@donto/client";
import { renderObject } from "@donto/client/history";

// vis-timeline is a DOM-only library; dynamic-import it inside useEffect
// so Next's RSC pass doesn't try to evaluate it on the server.

interface Props {
  rows: Statement[];
  subjectIri:    string;
  subjectLabel?: string | null;
  /** Called when an event is clicked. statementId from the row. */
  onSelect?: (statementId: string | null) => void;
}

/**
 * Horizontal timeline (vis.js / vis-timeline). Pan with drag, zoom with
 * scroll, click an item to select. One swim-lane per donto context.
 *
 * Mapping:
 *   item.id     = statement_id
 *   item.start  = valid_lo (rows without one are excluded —
 *                 tx_time would just be ingestion noise)
 *   item.end    = valid_hi (range item) — undefined → point item
 *   item.group  = context
 *   item.content= predicate (with object on hover via title)
 *   .className  = donto-{retracted|derived|asserted}
 */
export function Timeline({ rows, subjectIri, subjectLabel, onSelect }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const tlRef = useRef<{ destroy: () => void } | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Stable sorted distinct contexts → groups.
  const groups = useMemo(() => {
    const seen = new Map<string, number>();
    for (const r of rows) {
      const ctx = r.context ?? "(no context)";
      seen.set(ctx, (seen.get(ctx) ?? 0) + 1);
    }
    return [...seen.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([ctx, n], i) => {
        const safe = String(ctx ?? "");
        return {
          id: safe,
          content:
            `<div title="${escapeAttr(safe)}" style="font:11px ui-monospace,monospace">` +
            escapeHtml(safe.length > 36 ? safe.slice(0, 34) + "…" : safe) +
            ` <span style="color:#6a655e">${n}</span></div>`,
          order: i,
        };
      });
  }, [rows]);

  const items = useMemo(() => {
    return rows
      .map((r) => {
        // Time axis = valid_time (world-time). Rows without a real
        // valid_lo are excluded — see VerticalTimeline rationale.
        const tx_lo  = typeof r.tx_lo === "string" ? r.tx_lo : "";
        if (!r.valid_lo) return null;
        const start = new Date(r.valid_lo);
        if (Number.isNaN(start.getTime())) return null;
        const end = r.valid_hi ? new Date(r.valid_hi) : null;
        const obj = String(renderObject(r) ?? "");
        const lineage = Array.isArray(r.lineage) ? r.lineage : [];
        const ctx = r.context ?? "(no context)";
        const pred = r.predicate ?? "(no predicate)";
        const className =
          r.tx_hi ? "donto-retracted" :
          lineage.length ? "donto-derived" :
          "donto-asserted";
        return {
          id:    r.statement_id,
          group: ctx,
          start,
          ...(end && !Number.isNaN(end.getTime()) ? { end, type: "range" as const } : { type: "point" as const }),
          content:
            `<span class="donto-pred">${escapeHtml(pred)}</span>` +
            `<span class="donto-obj">${escapeHtml(obj.length > 60 ? obj.slice(0, 58) + "…" : obj)}</span>`,
          title:
            `${pred}\n${obj}\n` +
            `valid: ${r.valid_lo ?? "?"}` +
            (r.valid_hi ? ` → ${r.valid_hi}` : "") +
            (tx_lo ? `\nbelieved: ${tx_lo.slice(0,10)}` : "") +
            (r.tx_hi ? ` → ${String(r.tx_hi).slice(0,10)} (retracted)` : " → present"),
          className,
        };
      })
      .filter((x): x is NonNullable<typeof x> => x !== null);
  }, [rows]);

  useEffect(() => {
    if (!ref.current) return;
    let cancelled = false;
    let timelineApi: { destroy: () => void; setItems: (i: unknown) => void;
                       setGroups: (g: unknown) => void; on: (ev: string, fn: (p: unknown) => void) => void; } | null = null;

    (async () => {
      try {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const visTimeline: any = await import("vis-timeline/standalone");
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const visData: any     = await import("vis-data/peer");
        if (cancelled || !ref.current) return;
        const { Timeline: VTL } = visTimeline;
        const { DataSet }       = visData;

        const itemsDS  = new DataSet(items);
        const groupsDS = new DataSet(groups);

        const opts = {
          orientation: { axis: "top" as const, item: "top" as const },
          stack: true,
          stackSubgroups: true,
          horizontalScroll: true,
          zoomKey: "ctrlKey",
          margin: { item: 4, axis: 8 },
          height: "560px",
          minHeight: "560px",
          maxHeight: "560px",
          tooltip: { followMouse: true, overflowMethod: "cap" as const },
          format: {
            minorLabels: { day: "D", month: "MMM", year: "YYYY" },
          },
        };

        timelineApi = new VTL(ref.current, itemsDS, groupsDS, opts);
        tlRef.current = timelineApi as unknown as { destroy: () => void };
        timelineApi!.on("click", (props: unknown) => {
          const p = props as { item?: string | null };
          onSelect?.(p.item ?? null);
        });
      } catch (e) {
        if (cancelled) return;
        const msg = e instanceof Error ? e.message : String(e);
        setError(`vis-timeline failed to load: ${msg}`);
      }
    })();

    return () => {
      cancelled = true;
      try { tlRef.current?.destroy(); } catch { /* ignore */ }
      tlRef.current = null;
    };
  }, [items, groups, onSelect]);

  if (rows.length === 0) {
    return <div className="p-6 text-muted text-sm">no statements loaded</div>;
  }
  if (error) {
    return <div className="p-6 text-retract text-sm">{error}</div>;
  }

  return (
    <div>
      <div className="px-4 py-3 border-b border-rule">
        <div className="text-ink text-base">{subjectLabel ?? "(unlabelled)"}</div>
        <div className="text-muted text-[11px] break-all">
          {subjectIri} · {items.length.toLocaleString()} dated event{items.length === 1 ? "" : "s"}
          {rows.length > items.length && (
            <span className="text-retract">
              {" "}· {(rows.length - items.length).toLocaleString()} hidden (no valid_time)
            </span>
          )}
          {" · "}{groups.length} context{groups.length === 1 ? "" : "s"}
        </div>
        <div className="text-muted text-[10px] mt-0.5">
          time axis = valid_time (world-time) · drag to pan · ctrl+scroll to zoom · click to inspect
        </div>
      </div>
      <div ref={ref} className="donto-timeline" />
    </div>
  );
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[c]!);
}
function escapeAttr(s: string): string { return escapeHtml(s); }
