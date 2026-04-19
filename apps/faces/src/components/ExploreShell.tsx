"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import Link from "next/link";
import {
  donto,
  type DontoClient,
  type SearchMatch,
  type Statement,
} from "@donto/client";
import { renderObject } from "@donto/client/history";
import { Timeline } from "./Timeline";
import { VerticalTimeline } from "./VerticalTimeline";
import { StatementDetail } from "./StatementDetail";
import { fmtDate, makeColorMap } from "@/lib/colors";
import { useUrlState } from "@/lib/url-state";

interface Props { dontosrvUrl: string; }

const URL_DEFAULTS = {
  subject: "ex:darnell-brooks",
  view:    "vertical",
  order:   "newest",
  ctx:     "",
  pred:    "",
  sel:     "",
};

/**
 * ExploreShell — vertical / horizontal timeline view, sidebar, click-an-
 * item-see-everything detail drawer. All meaningful state is mirrored to
 * the URL so any view is shareable by copying the address bar.
 *
 * URL params:
 *   ?subject=<iri>     subject IRI in scope
 *   ?view=vertical|horizontal
 *   ?order=newest|oldest
 *   ?ctx=<context>     context filter
 *   ?pred=<predicate>  predicate filter
 *   ?sel=<uuid>        opens the StatementDetail drawer
 */
export function ExploreShell(props: Props) {
  // useSearchParams must be inside a Suspense boundary in Next 15+/16.
  return (
    <Suspense fallback={<div className="p-6 text-muted text-xs">loading…</div>}>
      <ExploreInner {...props} />
    </Suspense>
  );
}

function ExploreInner({ dontosrvUrl }: Props) {
  const client = useMemo<DontoClient>(() => donto(dontosrvUrl), [dontosrvUrl]);
  const colorOf = useMemo(() => makeColorMap(), []);

  const [params, setParams] = useUrlState(URL_DEFAULTS);
  const subject         = params.subject || URL_DEFAULTS.subject;
  const view            = (params.view === "horizontal" ? "horizontal" : "vertical") as "vertical" | "horizontal";
  const newestFirst     = params.order !== "oldest";
  const filterContext   = params.ctx ?? "";
  const filterPredicate = params.pred ?? "";
  const selected        = params.sel || null;

  const [rows,   setRows]   = useState<Statement[]>([]);
  const [total,  setTotal]  = useState(0);
  const [status, setStatus] = useState("loading…");

  // Search box (local state — only the picked subject hits the URL).
  const [searchQ,    setSearchQ]    = useState("");
  const [searchHits, setSearchHits] = useState<SearchMatch[]>([]);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searching,  setSearching]  = useState(false);

  // Load subject history.
  useEffect(() => {
    let cancelled = false;
    setStatus(`loading ${subject}…`);
    (async () => {
      try {
        const r = await client.history(subject, { limit: 5000 });
        if (cancelled) return;
        setRows(r.rows);
        setTotal(r.total);
        setStatus(
          r.rows.length === 0
            ? `no statements about ${subject}`
            : r.truncated
              ? `${r.rows.length.toLocaleString()} of ${r.total.toLocaleString()} (truncated)`
              : `${r.rows.length.toLocaleString()} statement${r.rows.length === 1 ? "" : "s"}`,
        );
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setStatus(`load failed: ${msg}`);
      }
    })();
    return () => { cancelled = true; };
  }, [client, subject]);

  // Debounced search.
  useEffect(() => {
    const q = searchQ.trim();
    if (q.length < 2) { setSearchHits([]); setSearching(false); return; }
    setSearching(true);
    const handle = setTimeout(async () => {
      try {
        const r = await client.search(q, 25);
        setSearchHits(r.matches);
      } finally {
        setSearching(false);
      }
    }, 220);
    return () => clearTimeout(handle);
  }, [searchQ, client]);

  // ── State setters that flow through the URL ───────────────────────────
  const goToSubject = useCallback((iri: string) => {
    setParams({ subject: iri, ctx: "", pred: "", sel: "" });
  }, [setParams]);
  const setView = useCallback((v: "vertical" | "horizontal") => {
    setParams({ view: v });
  }, [setParams]);
  const setOrder = useCallback((newest: boolean) => {
    setParams({ order: newest ? "newest" : "oldest" });
  }, [setParams]);
  const setFilterContext = useCallback((c: string) => {
    setParams({ ctx: c });
  }, [setParams]);
  const setFilterPredicate = useCallback((p: string) => {
    setParams({ pred: p });
  }, [setParams]);
  const setSelected = useCallback((id: string | null) => {
    setParams({ sel: id ?? "" });
  }, [setParams]);

  const pickSearchHit = useCallback((m: SearchMatch) => {
    goToSubject(m.subject);
    setSearchQ(m.label ?? m.subject);
    setSearchOpen(false);
  }, [goToSubject]);

  // ── Derived views over the row set ─────────────────────────────────────
  const contexts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const r of rows) counts.set(r.context, (counts.get(r.context) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1]);
  }, [rows]);

  const predicates = useMemo(() => {
    const counts = new Map<string, number>();
    for (const r of rows) counts.set(r.predicate, (counts.get(r.predicate) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1]);
  }, [rows]);

  const labels = useMemo(() => {
    return rows.filter((r) =>
      r.predicate === "rdfs:label" || r.predicate === "ex:label" || r.predicate === "ex:name"
    );
  }, [rows]);

  const related = useMemo(() => {
    const counts = new Map<string, { iri: string; predicates: Set<string>; count: number }>();
    for (const r of rows) {
      if (!r.object_iri) continue;
      if (r.object_iri.startsWith("_:") || r.object_iri.startsWith("xsd:")) continue;
      const e = counts.get(r.object_iri) ?? { iri: r.object_iri, predicates: new Set(), count: 0 };
      e.predicates.add(r.predicate);
      e.count++;
      counts.set(r.object_iri, e);
    }
    return [...counts.values()]
      .filter((e) => e.iri !== subject)
      .sort((a, b) => b.count - a.count)
      .slice(0, 30);
  }, [rows, subject]);

  const headlineLabel = useMemo<string | null>(() => {
    const ll = labels
      .map((r) => (r.object_lit && (r.object_lit.v as string)) || null)
      .filter((v): v is string => typeof v === "string");
    if (ll.length === 0) return null;
    const freq = new Map<string, number>();
    for (const v of ll) freq.set(v, (freq.get(v) ?? 0) + 1);
    return [...freq.entries()].sort((a, b) => b[1] - a[1])[0]![0];
  }, [labels]);

  const filteredRows = useMemo(() => {
    return rows.filter((r) =>
      (!filterContext   || r.context   === filterContext) &&
      (!filterPredicate || r.predicate === filterPredicate)
    );
  }, [rows, filterContext, filterPredicate]);

  return (
    <div className="min-h-screen flex flex-col">
      <header className="flex items-center gap-3 px-4 py-2 border-b border-rule bg-panel flex-wrap">
        <Link href="/" className="text-muted text-xs hover:text-accent">← faces</Link>
        <h1 className="text-accent text-sm tracking-wider m-0">donto · explore</h1>
        <span className="text-muted text-[11px]">→ {client.baseUrl}</span>
        <div className="relative">
          <input
            className="bg-paper border border-rule text-ink px-2 py-1 text-xs min-w-[360px]"
            placeholder="search by name (e.g. edward herbert) or paste IRI"
            value={searchQ}
            onChange={(e) => { setSearchQ(e.target.value); setSearchOpen(true); }}
            onFocus={() => setSearchOpen(true)}
            onBlur={() => setTimeout(() => setSearchOpen(false), 150)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                const v = (e.target as HTMLInputElement).value.trim();
                if (!v) return;
                if (searchHits.length === 1) pickSearchHit(searchHits[0]!);
                else goToSubject(v);
                setSearchOpen(false);
              } else if (e.key === "Escape") setSearchOpen(false);
            }}
          />
          {searchOpen && searchQ.trim().length >= 2 && (
            <div className="absolute z-40 left-0 mt-1 w-[460px] max-h-[320px] overflow-auto
                            bg-panel border border-rule text-xs shadow-xl">
              {searching && <div className="px-3 py-2 text-muted">searching…</div>}
              {!searching && searchHits.length === 0 && (
                <div className="px-3 py-2 text-muted">no matches</div>
              )}
              {searchHits.map((m) => (
                <button
                  key={m.subject}
                  type="button"
                  onMouseDown={(e) => { e.preventDefault(); pickSearchHit(m); }}
                  className="block w-full text-left px-3 py-1.5 hover:bg-rule
                             border-b border-rule/50 last:border-b-0"
                >
                  <div className="text-ink">{m.label ?? "(no label)"}</div>
                  <div className="text-muted text-[10px]">
                    {m.subject} · {m.count} statement{m.count === 1 ? "" : "s"}
                  </div>
                </button>
              ))}
            </div>
          )}
        </div>
        <div className="flex border border-rule">
          <button
            type="button"
            onClick={() => setView("vertical")}
            className={`px-2 py-1 text-[11px] ${view === "vertical"
              ? "bg-accent text-paper" : "bg-paper text-muted hover:text-ink"}`}
            title="vertical timeline (default)"
          >▤ vertical</button>
          <button
            type="button"
            onClick={() => setView("horizontal")}
            className={`px-2 py-1 text-[11px] border-l border-rule ${view === "horizontal"
              ? "bg-accent text-paper" : "bg-paper text-muted hover:text-ink"}`}
            title="horizontal swim-lane timeline"
          >▥ horizontal</button>
        </div>
        {view === "vertical" && (
          <button
            type="button"
            onClick={() => setOrder(!newestFirst)}
            className="bg-paper border border-rule text-muted hover:text-ink px-2 py-1 text-[11px]"
            title="flip chronological order"
          >{newestFirst ? "↑ newest" : "↓ oldest"}</button>
        )}
        <button
          type="button"
          onClick={() => navigator.clipboard?.writeText(window.location.href)}
          className="bg-paper border border-rule text-muted hover:text-ink px-2 py-1 text-[11px]"
          title="copy shareable URL"
        >🔗 copy link</button>
        <span className="text-muted text-xs ml-auto">{status}</span>
      </header>

      <div className="grid grid-cols-[1fr_360px] flex-1 min-h-0">
        <div className={view === "vertical" ? "min-h-0 overflow-hidden" : "overflow-auto"}>
          {filteredRows.length === 0
            ? <div className="p-6 text-muted text-sm">
                {rows.length === 0
                  ? "no statements loaded"
                  : "no statements match the current filters"}
              </div>
            : view === "vertical"
              ? <VerticalTimeline
                  rows={filteredRows}
                  subjectIri={subject}
                  subjectLabel={headlineLabel ?? undefined}
                  newestFirst={newestFirst}
                  onSelect={setSelected}
                />
              : <Timeline
                  rows={filteredRows}
                  subjectIri={subject}
                  subjectLabel={headlineLabel ?? undefined}
                  onSelect={setSelected}
                />}
        </div>

        <aside className="border-l border-rule bg-panel overflow-auto p-4 space-y-5 text-xs">
          <Section title="subject">
            <div className="text-ink text-[13px]">{headlineLabel ?? "(unlabelled)"}</div>
            <div className="text-muted text-[10px] break-all">{subject}</div>
            <div className="text-muted text-[10px] mt-1">
              {rows.length.toLocaleString()} of {total.toLocaleString()} statements loaded
            </div>
          </Section>

          {labels.length > 1 && (
            <Section title={`labels · ${labels.length}`}>
              <div className="space-y-1">
                {labels.slice(0, 12).map((r) => (
                  <button key={r.statement_id}
                    onClick={() => setSelected(r.statement_id)}
                    className="block text-left leading-tight w-full hover:bg-rule px-2 py-0.5"
                  >
                    <span className="text-ink">
                      “{r.object_lit && (r.object_lit.v as string)}”
                    </span>{" "}
                    <span className="text-muted text-[10px]">— {r.context}</span>
                    {r.tx_hi && <span className="text-retract"> · retracted</span>}
                  </button>
                ))}
                {labels.length > 12 && (
                  <div className="text-muted text-[10px]">… +{labels.length - 12} more</div>
                )}
              </div>
            </Section>
          )}

          <Section title={`contexts · ${contexts.length}`}>
            <ul className="space-y-0.5">
              {contexts.map(([c, n]) => (
                <li key={c}>
                  <button
                    onClick={() => setFilterContext(filterContext === c ? "" : c)}
                    className={`block w-full text-left px-2 py-0.5 hover:bg-rule
                                ${filterContext === c ? "bg-rule" : ""}`}
                    style={{ borderLeft: `2px solid ${colorOf(c)}` }}
                  >
                    <span className="text-ink">
                      {c.length > 36 ? c.slice(0, 34) + "…" : c}
                    </span>
                    <span className="text-muted ml-2">{n}</span>
                  </button>
                </li>
              ))}
            </ul>
          </Section>

          <Section title={`predicates · ${predicates.length}`}>
            <ul className="space-y-0.5">
              {predicates.slice(0, 30).map(([p, n]) => (
                <li key={p}>
                  <button
                    onClick={() => setFilterPredicate(filterPredicate === p ? "" : p)}
                    className={`block w-full text-left px-2 py-0.5 hover:bg-rule
                                ${filterPredicate === p ? "bg-rule" : ""}`}
                  >
                    <span className="text-ink">{p}</span>
                    <span className="text-muted ml-2">{n}</span>
                  </button>
                </li>
              ))}
              {predicates.length > 30 && (
                <li className="text-muted text-[10px] px-2 py-0.5">
                  … +{predicates.length - 30} more
                </li>
              )}
            </ul>
          </Section>

          {related.length > 0 && (
            <Section title={`linked-to subjects · ${related.length}`}>
              <ul className="space-y-0.5">
                {related.map((r) => (
                  <li key={r.iri}>
                    <button
                      onClick={() => goToSubject(r.iri)}
                      className="block w-full text-left px-2 py-0.5 hover:bg-rule"
                    >
                      <div className="text-ink truncate">{r.iri}</div>
                      <div className="text-muted text-[10px]">
                        via {[...r.predicates].slice(0, 3).join(", ")}
                        {r.predicates.size > 3 ? ", …" : ""}
                        {" · "}{r.count} ref{r.count === 1 ? "" : "s"}
                      </div>
                    </button>
                  </li>
                ))}
              </ul>
            </Section>
          )}

          {(filterContext || filterPredicate) && (
            <button
              className="bg-paper border border-rule text-ink px-3 py-1 hover:bg-rule w-full"
              onClick={() => { setFilterContext(""); setFilterPredicate(""); }}
            >clear filters</button>
          )}

          <Section title="recent statements">
            <ul className="space-y-1">
              {[...rows]
                .sort((a, b) => Date.parse(b.tx_lo) - Date.parse(a.tx_lo))
                .slice(0, 6)
                .map((r) => (
                <li key={r.statement_id}>
                  <button
                    onClick={() => setSelected(r.statement_id)}
                    className="block text-left leading-tight w-full hover:bg-rule px-2 py-0.5"
                  >
                    <div className="text-ink">{r.predicate}</div>
                    <div className="text-muted truncate">{renderObject(r)}</div>
                    <div className="text-muted text-[10px]">
                      believed {fmtDate(Date.parse(r.tx_lo))}
                      {r.tx_hi ? ` → ${fmtDate(Date.parse(r.tx_hi))} (retracted)` : ""}
                    </div>
                  </button>
                </li>
              ))}
            </ul>
          </Section>
        </aside>
      </div>

      <StatementDetail
        dontosrvUrl={dontosrvUrl}
        statementId={selected}
        onClose={() => setSelected(null)}
        onSelect={(id) => setSelected(id)}
        onPickSubject={(iri) => goToSubject(iri)}
      />
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h3 className="text-muted text-[10px] uppercase tracking-[0.14em] mb-1.5">{title}</h3>
      {children}
    </section>
  );
}
