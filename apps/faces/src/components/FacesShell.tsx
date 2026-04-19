"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  donto,
  type DontoClient,
  type SearchMatch,
  type Statement,
  type SubjectsResponse,
} from "@donto/client";
import { cubeBounds, distinctContexts, type CubePoint } from "@donto/client/history";
import { Stratigraph } from "./Stratigraph";
import { Rashomon } from "./Rashomon";
import { Probe } from "./Probe";
import { RowStream } from "./RowStream";
import { makeColorMap } from "@/lib/colors";

interface Props { dontosrvUrl: string; }

// Verbose logger. Mirrors to console AND an in-page log panel so the user
// can read what's happening without opening devtools. Guards the "is it
// loading or hung?" failure mode where state goes silent.
type LogEvent = { t: number; kind: "info" | "ok" | "err"; msg: string };
function fmtClock(t: number): string {
  const d = new Date(t);
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map((n) => String(n).padStart(2, "0")).join(":") +
    "." + String(d.getMilliseconds()).padStart(3, "0");
}

export function FacesShell({ dontosrvUrl }: Props) {
  const client = useMemo<DontoClient>(() => donto(dontosrvUrl), [dontosrvUrl]);
  const colorOf = useRef(makeColorMap()).current;

  const [subjects, setSubjects] = useState<SubjectsResponse["subjects"]>([]);
  const [subject, setSubject] = useState<string | null>(null);
  const [rows,    setRows]    = useState<Statement[]>([]);
  const [total,   setTotal]   = useState(0);
  const [truncated, setTruncated] = useState(false);
  const [status,  setStatus]  = useState("booting…");
  const [errorDetail, setErrorDetail] = useState<string | null>(null);
  const [cursor,  setCursor]  = useState<CubePoint | null>(null);
  const [log,     setLog]     = useState<LogEvent[]>([]);
  const [bootTick, setBootTick] = useState(0);
  // Search box state: query string + matches + open-dropdown flag.
  const [searchQ,    setSearchQ]    = useState("");
  const [searchHits, setSearchHits] = useState<SearchMatch[]>([]);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searching,  setSearching]  = useState(false);
  // Server-side filters.
  const [filterContext,   setFilterContext]   = useState<string>("");
  const [filterPredicate, setFilterPredicate] = useState<string>("");
  const [limit,           setLimit]           = useState<number>(2000);

  const logEvent = useCallback((kind: LogEvent["kind"], msg: string) => {
    const ev = { t: Date.now(), kind, msg };
    // eslint-disable-next-line no-console
    console[kind === "err" ? "error" : "log"](`[faces ${fmtClock(ev.t)}]`, msg);
    setLog((prev) => [...prev.slice(-99), ev]);
  }, []);

  // Boot log: show exactly what URL the client is pointing at.
  useEffect(() => {
    logEvent("info", `dontosrvUrl prop = ${dontosrvUrl}`);
    logEvent("info", `client.baseUrl   = ${client.baseUrl}`);
    if (typeof window !== "undefined") {
      logEvent("info", `window.location  = ${window.location.href}`);
    }
  }, [client, dontosrvUrl, logEvent]);

  // Bootstrap: probe /health, then load the default subject IMMEDIATELY,
  // and fetch /subjects in parallel (so a slow picker doesn't block the
  // page from rendering anything useful).
  useEffect(() => {
    let cancelled = false;
    (async () => {
      setErrorDetail(null);
      setStatus("probing /health…");
      logEvent("info", `GET ${client.baseUrl}/health`);
      const t0 = performance.now();
      try {
        const r = await fetch(`${client.baseUrl}/health`);
        if (cancelled) return;
        if (!r.ok) {
          logEvent("err", `/health → ${r.status} ${r.statusText}`);
          setStatus(`/health returned ${r.status}`);
          setErrorDetail(`GET ${client.baseUrl}/health → HTTP ${r.status} ${r.statusText}`);
          return;
        }
        const body = await r.text();
        logEvent("ok", `/health ok in ${Math.round(performance.now()-t0)}ms → "${body.trim()}"`);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        logEvent("err", `/health threw: ${msg}`);
        setStatus("cannot reach dontosrv");
        setErrorDetail(
          `fetch to ${client.baseUrl}/health threw:\n  ${msg}\n\n` +
          `likely causes:\n` +
          `  • dontosrv isn't running on that URL (try: cargo run -p dontosrv)\n` +
          `  • NEXT_PUBLIC_DONTOSRV_URL is wrong; currently "${client.baseUrl}"\n` +
          `  • CORS missing — dontosrv must include\n` +
          `      access-control-allow-origin: *\n` +
          `    on responses to this origin (${typeof window !== "undefined" ? window.location.origin : "?"}).`,
        );
        return;
      }
      if (cancelled) return;

      // Default subject — render something immediately. The picker comes
      // later, in parallel, so a slow /subjects doesn't block the page.
      if (!subject) {
        setSubject("ex:darnell-brooks");
        logEvent("info", "default subject: ex:darnell-brooks");
      }

      // Fire /subjects in the background.
      logEvent("info", `GET ${client.baseUrl}/subjects (background)`);
      const t1 = performance.now();
      client.subjects().then((r) => {
        if (cancelled) return;
        logEvent("ok", `/subjects ok in ${Math.round(performance.now()-t1)}ms → ${r.subjects.length} subjects`);
        const ordered = r.subjects.slice().sort((a, b) => {
          if (a.subject === "ex:darnell-brooks") return -1;
          if (b.subject === "ex:darnell-brooks") return  1;
          return b.count - a.count;
        });
        setSubjects(ordered);
      }).catch((e) => {
        if (cancelled) return;
        const msg = e instanceof Error ? e.message : String(e);
        logEvent("err", `/subjects failed (non-fatal): ${msg}`);
      });
    })();
    return () => { cancelled = true; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, logEvent, bootTick]);

  // Load the selected subject's history. Re-runs on filter / limit change.
  useEffect(() => {
    if (!subject) return;
    let cancelled = false;
    (async () => {
      setStatus(`loading history for ${subject}…`);
      const q = {
        limit,
        ...(filterContext   ? { context:   filterContext   } : {}),
        ...(filterPredicate ? { predicate: filterPredicate } : {}),
      };
      logEvent("info", `GET ${client.baseUrl}/history/${subject} ${JSON.stringify(q)}`);
      const t0 = performance.now();
      try {
        const r = await client.history(subject, q);
        if (cancelled) return;
        logEvent("ok",
          `/history ok in ${Math.round(performance.now()-t0)}ms → ${r.rows.length} of ${r.total} rows`);
        setRows(r.rows);
        setTotal(r.total);
        setTruncated(r.truncated);
        setStatus(
          r.rows.length === 0
            ? `no statements about ${subject}`
            : r.truncated
              ? `${r.rows.length.toLocaleString()} of ${r.total.toLocaleString()} (truncated; raise limit or filter)`
              : `${r.rows.length.toLocaleString()} statement${r.rows.length === 1 ? "" : "s"}`,
        );
        if (r.rows.length) {
          const b = cubeBounds(r.rows);
          setCursor({ valid: (b.vMin + b.vMax) / 2, tx: Math.min(b.tMax, Date.now()) });
        } else {
          setCursor(null);
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        logEvent("err", `/history failed: ${msg}`);
        setStatus("/history failed");
        setErrorDetail(`/history/${subject} failed:\n  ${msg}`);
      }
    })();
    return () => { cancelled = true; };
  }, [client, subject, logEvent, filterContext, filterPredicate, limit]);

  // Reset filters when the subject changes (otherwise we'd query for a
  // context that doesn't exist on the new subject).
  useEffect(() => {
    setFilterContext("");
    setFilterPredicate("");
    setLimit(2000);
  }, [subject]);

  // Debounced /search as the user types in the search box.
  useEffect(() => {
    const q = searchQ.trim();
    if (q.length < 2) { setSearchHits([]); setSearching(false); return; }
    setSearching(true);
    const handle = setTimeout(async () => {
      try {
        const r = await client.search(q, 25);
        setSearchHits(r.matches);
        logEvent("ok", `/search "${q}" → ${r.matches.length} matches`);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        logEvent("err", `/search "${q}" failed: ${msg}`);
      } finally {
        setSearching(false);
      }
    }, 220);
    return () => clearTimeout(handle);
  }, [searchQ, client, logEvent]);

  const pickSearchHit = useCallback((m: SearchMatch) => {
    setSubject(m.subject);
    setSearchQ(m.label ?? m.subject);
    setSearchOpen(false);
  }, []);

  const bounds   = useMemo(() => cubeBounds(rows), [rows]);
  const contexts = useMemo(() => distinctContexts(rows), [rows]);
  const predicates = useMemo(() => {
    const seen = new Map<string, number>();
    for (const r of rows) seen.set(r.predicate, (seen.get(r.predicate) ?? 0) + 1);
    return [...seen.entries()].sort((a, b) => b[1] - a[1]);
  }, [rows]);

  return (
    <div className="h-screen flex flex-col">
      <header className="flex items-center gap-3 px-4 py-2 border-b border-rule bg-panel flex-wrap">
        <h1 className="text-accent text-sm tracking-wider m-0">donto · faces</h1>
        <a href="/explore" className="text-muted text-xs hover:text-accent">explore →</a>
        <span className="text-muted text-[11px]">→ {client.baseUrl}</span>
        <select
          className="bg-paper border border-rule text-ink px-2 py-1 text-xs min-w-[280px]"
          value={subject ?? ""}
          onChange={(e) => setSubject(e.target.value)}
          disabled={subjects.length === 0}
        >
          {subjects.length === 0 && <option>(no subjects)</option>}
          {subjects.map((s) => (
            <option key={s.subject} value={s.subject}>
              {s.subject} ({s.count})
            </option>
          ))}
        </select>
        <div className="relative">
          <input
            className="bg-paper border border-rule text-ink px-2 py-1 text-xs min-w-[320px]"
            placeholder="search by name (e.g. edward herbert) or paste IRI + ↵"
            value={searchQ}
            onChange={(e) => { setSearchQ(e.target.value); setSearchOpen(true); }}
            onFocus={() => setSearchOpen(true)}
            onBlur={() => setTimeout(() => setSearchOpen(false), 150)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                const v = (e.target as HTMLInputElement).value.trim();
                if (!v) return;
                // If exactly-one hit, pick it. Otherwise treat input as
                // an IRI (works whether the picker matched or not).
                if (searchHits.length === 1) pickSearchHit(searchHits[0]!);
                else setSubject(v);
                setSearchOpen(false);
              } else if (e.key === "Escape") {
                setSearchOpen(false);
              }
            }}
          />
          {searchOpen && (searchQ.trim().length >= 2) && (
            <div className="absolute z-40 left-0 mt-1 w-[460px] max-h-[320px] overflow-auto
                            bg-panel border border-rule text-xs shadow-xl">
              {searching && (
                <div className="px-3 py-2 text-muted">searching…</div>
              )}
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
        <button
          className="bg-paper border border-rule text-ink text-xs px-3 py-1 hover:bg-rule"
          onClick={() => { setErrorDetail(null); setBootTick((t) => t + 1); }}
        >retry</button>
        <span className="text-muted text-xs ml-auto">{status}</span>
      </header>

      {errorDetail && (
        <div className="px-4 py-2 border-b border-retract/60 bg-retract/10 text-retract text-xs whitespace-pre-wrap">
          {errorDetail}
        </div>
      )}

      {/* Filter bar — shows once a subject is loaded. */}
      {subject && rows.length > 0 && (
        <div className="px-4 py-2 border-b border-rule bg-paper flex items-center gap-3 text-[11px] flex-wrap">
          <span className="text-muted">filter:</span>
          <select
            className="bg-panel border border-rule text-ink px-2 py-1"
            value={filterContext}
            onChange={(e) => setFilterContext(e.target.value)}
          >
            <option value="">all contexts ({contexts.length})</option>
            {contexts.map((c) => (
              <option key={c} value={c}>{c}</option>
            ))}
          </select>
          <select
            className="bg-panel border border-rule text-ink px-2 py-1"
            value={filterPredicate}
            onChange={(e) => setFilterPredicate(e.target.value)}
          >
            <option value="">all predicates ({predicates.length})</option>
            {predicates.map(([p, n]) => (
              <option key={p} value={p}>{p} ({n})</option>
            ))}
          </select>
          <span className="text-muted">limit:</span>
          <select
            className="bg-panel border border-rule text-ink px-2 py-1"
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value))}
          >
            {[500, 2000, 5000, 10000, 20000].map((n) => (
              <option key={n} value={n}>{n.toLocaleString()}</option>
            ))}
          </select>
          {(filterContext || filterPredicate) && (
            <button
              className="bg-panel border border-rule text-ink px-2 py-1 hover:bg-rule"
              onClick={() => { setFilterContext(""); setFilterPredicate(""); }}
            >clear filters</button>
          )}
          {truncated && (
            <span className="ml-auto text-accent">
              ⚠ showing {rows.length.toLocaleString()} of {total.toLocaleString()} —
              use filters or raise the limit
            </span>
          )}
        </div>
      )}

      <main className="flex-1 grid grid-rows-[minmax(0,1fr)_minmax(0,1fr)_140px] min-h-0">
        <div className="grid grid-cols-2 min-h-0">
          <Panel title="stratigraph" legend="x: valid_time · y: tx_time (down = older belief)">
            <Stratigraph rows={rows} bounds={bounds} colorOf={colorOf} />
          </Panel>
          <Panel title="rashomon hall" legend="lanes: contexts · red lines: same predicate, diverging objects">
            <Rashomon rows={rows} bounds={bounds} contexts={contexts} colorOf={colorOf} />
          </Panel>
        </div>
        <div className="grid grid-cols-2 min-h-0">
          <Panel title="probe" legend="click anywhere · cursor = (valid_time, tx_time)">
            <Probe rows={rows} bounds={bounds} cursor={cursor} onCursor={setCursor} colorOf={colorOf} />
          </Panel>
          <Panel title="row stream" legend="every statement, every polarity, no resolution">
            <RowStream rows={rows} colorOf={colorOf} />
          </Panel>
        </div>
        <section className="border-t border-rule bg-panel text-[11px] overflow-auto px-3 py-1.5 font-mono">
          <div className="text-muted mb-1">
            debug log · {log.length} event{log.length === 1 ? "" : "s"}
          </div>
          {log.length === 0 ? (
            <div className="text-muted/60">(no events yet)</div>
          ) : log.map((e, i) => (
            <div key={i} className={
              e.kind === "err" ? "text-retract" :
              e.kind === "ok"  ? "text-derived"  :
              "text-ink/70"
            }>
              <span className="text-muted">{fmtClock(e.t)}</span>{"  "}{e.msg}
            </div>
          ))}
        </section>
      </main>
    </div>
  );
}

function Panel({
  title, legend, children,
}: { title: string; legend: string; children: React.ReactNode }) {
  return (
    <section className="border border-rule m-1.5 bg-panel flex flex-col overflow-hidden">
      <h2 className="m-0 px-3 py-2 text-[11px] font-medium uppercase tracking-[0.12em]
                     text-muted border-b border-rule flex items-center justify-between">
        {title}
        <span className="text-muted normal-case tracking-normal font-normal">
          {legend}
        </span>
      </h2>
      <div className="flex-1 min-h-0 overflow-hidden relative">{children}</div>
    </section>
  );
}
