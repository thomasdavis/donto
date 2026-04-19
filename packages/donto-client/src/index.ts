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
  subject:   string;
  count:     number;
  total:     number;
  truncated: boolean;
  limit:     number;
  filters: {
    context:   string | null;
    predicate: string | null;
    from:      string | null;
    to:        string | null;
    include_retracted: boolean;
  };
  rows:    Statement[];
}

export interface HistoryQuery {
  limit?:     number;
  context?:   string;
  predicate?: string;
  from?:      string; // ISO date
  to?:        string; // ISO date
  include_retracted?: boolean;
}

export interface SubjectsResponse {
  subjects: { subject: string; count: number }[];
}

export interface SearchMatch {
  subject: string;
  label:   string | null;
  count:   number;
}
export interface SearchResponse {
  q: string;
  matches: SearchMatch[];
}

export interface DontoClient {
  /** Base URL the client points at. */
  readonly baseUrl: string;
  history(subject: string, q?: HistoryQuery): Promise<HistoryResponse>;
  subjects(): Promise<SubjectsResponse>;
  search(q: string, limit?: number): Promise<SearchResponse>;
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
    history: (s, q) => {
      const params = new URLSearchParams();
      if (q?.limit != null)     params.set("limit", String(q.limit));
      if (q?.context)           params.set("context", q.context);
      if (q?.predicate)         params.set("predicate", q.predicate);
      if (q?.from)              params.set("from", q.from);
      if (q?.to)                params.set("to", q.to);
      if (q?.include_retracted != null)
        params.set("include_retracted", String(q.include_retracted));
      const qs = params.toString();
      return get<HistoryResponse>(
        `/history/${encodeURIComponent(s)}` + (qs ? `?${qs}` : "")
      );
    },
    subjects: () => get<SubjectsResponse>(`/subjects`),
    search:   (q, limit) => {
      const url = `/search?q=${encodeURIComponent(q)}` +
                  (limit ? `&limit=${limit}` : "");
      return get<SearchResponse>(url);
    },
    health:   async () => {
      const r = await fetch(`${trimmed}/health`);
      return r.ok && (await r.text()) === "ok";
    },
    version:  () => get(`/version`),
  };
}
