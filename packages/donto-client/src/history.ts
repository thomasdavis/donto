/** Subject-history helpers and bitemporal cube types. */
export type { Statement, HistoryResponse } from "./index.js";

import type { Statement } from "./index.js";

/**
 * A single point in donto's bitemporal cube.
 *  valid: ms-since-epoch (when the world said it was true)
 *  tx:    ms-since-epoch (when donto believed it)
 */
export interface CubePoint {
  valid: number;
  tx:    number;
}

/** Was this statement live at the given cube point? */
export function isLiveAt(s: Statement, p: CubePoint, now: number = Date.now()): boolean {
  const vlo = s.valid_lo ? Date.parse(s.valid_lo) : -Infinity;
  const vhi = s.valid_hi ? Date.parse(s.valid_hi) :  Infinity;
  const tlo = s.tx_lo ? Date.parse(s.tx_lo) : 0;
  const thi = s.tx_hi ? Date.parse(s.tx_hi) : now + 1;
  return vlo <= p.valid && p.valid <= vhi && tlo <= p.tx && p.tx <= thi;
}

/** Compute the bounding rectangle in (valid_time, tx_time) for a row set. */
export function cubeBounds(rows: Statement[]): {
  vMin: number; vMax: number; tMin: number; tMax: number;
} {
  let vMin = Infinity, vMax = -Infinity, tMin = Infinity, tMax = -Infinity;
  for (const r of rows) {
    const vlo = r.valid_lo ? Date.parse(r.valid_lo) : null;
    const vhi = r.valid_hi ? Date.parse(r.valid_hi) : null;
    const tlo = r.tx_lo ? Date.parse(r.tx_lo) : null;
    const thi = r.tx_hi ? Date.parse(r.tx_hi) : null;
    if (vlo != null) { vMin = Math.min(vMin, vlo); vMax = Math.max(vMax, vlo); }
    if (vhi != null) { vMin = Math.min(vMin, vhi); vMax = Math.max(vMax, vhi); }
    if (tlo != null) { tMin = Math.min(tMin, tlo); tMax = Math.max(tMax, tlo); }
    if (thi != null) { tMin = Math.min(tMin, thi); tMax = Math.max(tMax, thi); }
  }
  const NOW = Date.now();
  if (!isFinite(vMin)) { vMin = NOW - 30 * 365 * 86400_000; vMax = NOW; }
  if (!isFinite(tMin)) { tMin = NOW -  5 * 365 * 86400_000; tMax = NOW; }
  return { vMin, vMax, tMin, tMax };
}

/** Distinct contexts in a row set, stable-sorted by first appearance. */
export function distinctContexts(rows: Statement[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const r of rows) if (!seen.has(r.context)) { seen.add(r.context); out.push(r.context); }
  return out;
}

/** Render the object of a statement as a flat string (IRI or literal value). */
export function renderObject(s: Statement): string {
  if (s.object_iri) return s.object_iri;
  if (s.object_lit) {
    const v = s.object_lit.v;
    return typeof v === "string" ? v : JSON.stringify(v);
  }
  return "";
}
