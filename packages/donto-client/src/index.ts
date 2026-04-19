/**
 * @donto/client — TypeScript bindings for the dontosrv HTTP surface.
 *
 * Mirrors the Rust `donto-client` crate at the type level so apps in this
 * monorepo (apps/faces et al.) can talk to a running dontosrv with no
 * code generation step.
 *
 * Usage:
 *
 *   import { donto } from "@donto/client";
 *   const c = donto("http://localhost:7878");
 *   const history = await c.history("ex:darnell-brooks");
 */

export type Polarity = "asserted" | "negated" | "absent" | "unknown";

export interface Literal {
  v: unknown;
  dt: string;
  lang?: string | null;
}

/** A single statement in donto. Mirrors the SQL physical row. */
export interface Statement {
  statement_id: string;
  subject:   string;
  predicate: string;
  object_iri?: string | null;
  object_lit?: Literal | null;
  context:   string;
  polarity:  Polarity;
  maturity:  number;
  /** ISO date or null (= -infinity). */
  valid_lo?: string | null;
  /** ISO date or null (= +infinity). */
  valid_hi?: string | null;
  /** ISO timestamp; lower bound of tx_time. */
  tx_lo: string;
  /** ISO timestamp; null = still believed (open tx_time). */
  tx_hi?: string | null;
  /** statement_ids this row was derived from. */
  lineage: string[];
}

export interface HistoryResponse {
  subject: string;
  count:   number;
  rows:    Statement[];
}

export interface SubjectsResponse {
  subjects: { subject: string; count: number }[];
}

export interface DontoClient {
  /** Base URL the client points at. */
  readonly baseUrl: string;
  history(subject: string): Promise<HistoryResponse>;
  subjects(): Promise<SubjectsResponse>;
  health(): Promise<boolean>;
  version(): Promise<{ service: string; version: string; dir: string }>;
}

export function donto(baseUrl: string): DontoClient {
  const trimmed = baseUrl.replace(/\/+$/, "");
  async function get<T>(path: string): Promise<T> {
    const r = await fetch(`${trimmed}${path}`, {
      headers: { accept: "application/json" },
    });
    if (!r.ok) throw new Error(`dontosrv ${path}: ${r.status} ${r.statusText}`);
    return (await r.json()) as T;
  }
  return {
    baseUrl: trimmed,
    history: (s) => get<HistoryResponse>(`/history/${encodeURIComponent(s)}`),
    subjects: () => get<SubjectsResponse>(`/subjects`),
    health:   async () => {
      const r = await fetch(`${trimmed}/health`);
      return r.ok && (await r.text()) === "ok";
    },
    version:  () => get(`/version`),
  };
}
