"use client";

import type { Statement } from "@donto/client";
import { renderObject } from "@donto/client/history";
import { fmtDate } from "@/lib/colors";

/** The literal row stream — terminal-style, no resolution. */
export function RowStream({
  rows, colorOf,
}: { rows: Statement[]; colorOf: (ctx: string) => string }) {
  return (
    <div className="px-4 py-3 text-xs overflow-auto h-full">
      {rows.map((r) => {
        const vlo = r.valid_lo ? Date.parse(r.valid_lo) : null;
        const vhi = r.valid_hi ? Date.parse(r.valid_hi) : null;
        const tlo = Date.parse(r.tx_lo);
        const thi = r.tx_hi ? Date.parse(r.tx_hi) : null;
        return (
          <div
            key={r.statement_id}
            className="border-l-2 px-3 py-1.5 my-2 bg-[#181612]"
            style={{
              borderLeftColor: r.tx_hi ? "#b0584c" : (r.lineage.length ? "#82a45a" : colorOf(r.context)),
              opacity: r.tx_hi ? 0.6 : 1,
            }}
          >
            <div>
              <span className="text-accent font-medium">{r.context}</span>{"  "}
              <span>{r.predicate}</span>
            </div>
            <div className="text-ink/90">&quot;{renderObject(r)}&quot;</div>
            <div className="text-muted text-[11px]">
              valid {fmtDate(vlo)}
              {vhi ? `..${fmtDate(vhi)}` : ""}
              {" · "}
              believed {fmtDate(tlo)}
              {thi ? ` → ${fmtDate(thi)} (retracted)` : " → present"}
              {r.lineage.length > 0 && (
                <> · lineage: {r.lineage.length} input{r.lineage.length === 1 ? "" : "s"}</>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}
