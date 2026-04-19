"use client";

import { useEffect, useState } from "react";
import {
  donto,
  type AuditEntry,
  type CertificateInfo,
  type Statement,
  type StatementDetail as Detail,
} from "@donto/client";
import { renderObject } from "@donto/client/history";
import { fmtDate } from "@/lib/colors";

interface Props {
  dontosrvUrl: string;
  statementId: string | null;
  /** Called when the user closes the drawer. */
  onClose: () => void;
  /** Called when the user clicks a statement_id link inside the drawer. */
  onSelect?: (id: string) => void;
  /** Called when a subject IRI link is clicked (e.g. to navigate the page). */
  onPickSubject?: (subjectIri: string) => void;
}

/**
 * Drawer that shows EVERYTHING dontosrv knows about a single statement:
 *   * The full row (no truncation)
 *   * Bitemporal facts (valid_time + tx_time)
 *   * Lineage in BOTH directions (sources / derived)
 *   * Audit log entries (assert / retract / correct)
 *   * Certificate (if attached) with verifier verdict
 *   * Sibling statements (same subject + predicate, all polarities)
 */
export function StatementDetail({
  dontosrvUrl, statementId, onClose, onSelect, onPickSubject,
}: Props) {
  const [data,  setData]  = useState<Detail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!statementId) { setData(null); setError(null); return; }
    let cancelled = false;
    setLoading(true);
    setError(null);
    setData(null);
    (async () => {
      try {
        const c = donto(dontosrvUrl);
        const r = await c.statement(statementId);
        if (cancelled) return;
        setData(r);
      } catch (e) {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, [dontosrvUrl, statementId]);

  // Esc closes the drawer.
  useEffect(() => {
    if (!statementId) return;
    function key(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [statementId, onClose]);

  if (!statementId) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex justify-end bg-paper/70"
      onClick={onClose}
    >
      <aside
        className="w-[640px] max-w-[100vw] h-full overflow-auto bg-panel
                   border-l border-rule shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="px-4 py-3 border-b border-rule flex items-center gap-3 sticky top-0 bg-panel z-10">
          <h2 className="text-accent text-sm tracking-wider m-0">statement</h2>
          <code className="text-muted text-[10px] flex-1 truncate">{statementId}</code>
          <button
            onClick={onClose}
            className="text-muted hover:text-ink text-xs px-2 py-0.5 border border-rule"
            title="close (esc)"
          >×</button>
        </header>

        {loading && <div className="p-4 text-muted text-xs">loading…</div>}
        {error && (
          <div className="p-4 text-retract text-xs whitespace-pre-wrap">
            {error}
          </div>
        )}
        {data && (
          <DetailBody data={data} onSelect={onSelect} onPickSubject={onPickSubject} />
        )}
      </aside>
    </div>
  );
}

function DetailBody({
  data, onSelect, onPickSubject,
}: {
  data: Detail;
  onSelect?: (id: string) => void;
  onPickSubject?: (s: string) => void;
}) {
  const { statement: s, lineage, audit, certificate, siblings } = data;
  const isRetracted = !!s.tx_hi;
  const isDerived = lineage.sources.length > 0;
  const obj = String(renderObject(s) ?? "");

  return (
    <div className="p-4 space-y-5 text-xs">

      <Section title="row">
        <div className="space-y-1.5">
          <KV k="subject"    v={
            <button
              onClick={() => onPickSubject?.(s.subject)}
              className="text-accent hover:underline text-left break-all"
            >{s.subject}</button>
          } />
          <KV k="predicate"  v={<span className="text-accent">{s.predicate}</span>} />
          <KV k="object"     v={
            s.object_iri
              ? <button
                  onClick={() => onPickSubject?.(s.object_iri!)}
                  className="text-accent hover:underline break-all text-left"
                >{s.object_iri}</button>
              : <span className="break-all whitespace-pre-wrap">&quot;{obj}&quot;</span>
          } />
          {s.object_lit && (
            <KV k="datatype" v={
              <span className="text-muted">
                {(s.object_lit as { dt?: string }).dt ?? "(stringified literal)"}
                {(s.object_lit as { lang?: string | null }).lang
                  ? ` · @${(s.object_lit as { lang?: string }).lang}` : ""}
              </span>
            } />
          )}
          <KV k="context"    v={<span className="break-all">{s.context}</span>} />
          <KV k="polarity"   v={<span>{s.polarity}{isRetracted && (
            <span className="text-retract"> · retracted</span>
          )}</span>} />
          <KV k="maturity"   v={<span>{s.maturity}</span>} />
        </div>
      </Section>

      <Section title="bitemporal">
        <div className="space-y-1.5">
          <KV k="valid_time" v={
            s.valid_lo
              ? <span>{s.valid_lo}{s.valid_hi ? ` → ${s.valid_hi}` : " → ∞"}</span>
              : <span className="text-muted">(undated)</span>
          } />
          <KV k="tx_time"    v={
            <span>
              {fmtDate(Date.parse(s.tx_lo))}
              {s.tx_hi
                ? <> → {fmtDate(Date.parse(s.tx_hi))} <span className="text-retract">(closed)</span></>
                : <> → present</>}
            </span>
          } />
          <KV k="raw tx_lo"  v={<code className="text-muted">{s.tx_lo}</code>} />
          {s.tx_hi && <KV k="raw tx_hi" v={<code className="text-muted">{s.tx_hi}</code>} />}
        </div>
      </Section>

      <Section title={`lineage · ${lineage.sources.length} source · ${lineage.derived.length} derived`}>
        {!isDerived && lineage.derived.length === 0 && (
          <div className="text-muted">no lineage on either side</div>
        )}
        {lineage.sources.length > 0 && (
          <div>
            <div className="text-muted text-[10px] mb-1 uppercase tracking-wider">derived from</div>
            <div className="space-y-1">
              {lineage.sources.map((src) => (
                <RowLink key={src.statement_id} row={src} onSelect={onSelect} />
              ))}
            </div>
          </div>
        )}
        {lineage.derived.length > 0 && (
          <div className="mt-3">
            <div className="text-muted text-[10px] mb-1 uppercase tracking-wider">cited by</div>
            <div className="space-y-1">
              {lineage.derived.map((d) => (
                <RowLink key={d.statement_id} row={d} onSelect={onSelect} />
              ))}
            </div>
          </div>
        )}
      </Section>

      {certificate && (
        <Section title={`certificate · ${certificate.kind}`}>
          <CertificateBlock c={certificate} />
        </Section>
      )}

      {siblings.length > 0 && (
        <Section title={`siblings · same subject + predicate · ${siblings.length}`}>
          <div className="space-y-1">
            {siblings.map((sib) => (
              <RowLink key={sib.statement_id} row={sib} onSelect={onSelect} />
            ))}
          </div>
        </Section>
      )}

      {audit.length > 0 && (
        <Section title={`audit · ${audit.length}`}>
          <div className="space-y-1">
            {audit.map((e, i) => <AuditRow key={i} e={e} />)}
          </div>
        </Section>
      )}

      <Section title="raw json">
        <details>
          <summary className="text-muted cursor-pointer">expand</summary>
          <pre className="mt-2 text-[10px] text-muted whitespace-pre-wrap break-all
                          max-h-[400px] overflow-auto p-2 bg-paper border border-rule">
            {JSON.stringify(data, null, 2)}
          </pre>
        </details>
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section>
      <h3 className="text-muted text-[10px] uppercase tracking-[0.14em] mb-1.5">{title}</h3>
      <div>{children}</div>
    </section>
  );
}

function KV({ k, v }: { k: string; v: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[110px_1fr] gap-2 items-baseline">
      <div className="text-muted">{k}</div>
      <div className="text-ink">{v}</div>
    </div>
  );
}

function RowLink({ row, onSelect }: { row: Statement; onSelect?: (id: string) => void }) {
  const obj = renderObject(row);
  return (
    <button
      onClick={() => onSelect?.(row.statement_id)}
      className="block w-full text-left border-l-2 border-rule px-2 py-1
                 bg-paper hover:bg-rule"
    >
      <div className="text-[11px]">
        <span className="text-accent">{row.predicate}</span>
        {" → "}
        <span className="text-ink">{obj.length > 60 ? obj.slice(0, 58) + "…" : obj}</span>
      </div>
      <div className="text-muted text-[10px]">
        {row.context} · valid {row.valid_lo ?? "?"}
        {row.valid_hi ? `..${row.valid_hi}` : ""}
        {row.tx_hi && <span className="text-retract"> · retracted</span>}
      </div>
    </button>
  );
}

function CertificateBlock({ c }: { c: CertificateInfo }) {
  return (
    <div className="space-y-1">
      <KV k="kind"        v={<span className="text-accent">{c.kind}</span>} />
      {c.rule_iri    && <KV k="rule"        v={c.rule_iri} />}
      <KV k="produced_at" v={fmtDate(Date.parse(c.produced_at))} />
      <KV k="verified_ok" v={
        c.verified_ok == null
          ? <span className="text-muted">(not verified)</span>
          : c.verified_ok
            ? <span className="text-derived">✓ ok</span>
            : <span className="text-retract">✗ rejected</span>
      } />
      {c.inputs.length > 0 && (
        <KV k="inputs" v={
          <div className="space-y-0.5">
            {c.inputs.map((id) => (
              <code key={id} className="block text-muted text-[10px]">{id}</code>
            ))}
          </div>
        } />
      )}
      <details className="mt-1">
        <summary className="text-muted cursor-pointer text-[10px]">body</summary>
        <pre className="mt-1 text-[10px] text-muted whitespace-pre-wrap break-all
                        max-h-[200px] overflow-auto p-2 bg-paper border border-rule">
          {JSON.stringify(c.body, null, 2)}
        </pre>
      </details>
    </div>
  );
}

function AuditRow({ e }: { e: AuditEntry }) {
  return (
    <div className="border-l-2 border-rule px-2 py-1 bg-paper">
      <div className="text-[11px]">
        <span className="text-accent">{e.action}</span>
        <span className="text-muted ml-2">{fmtDate(Date.parse(e.at))}</span>
        {e.actor && <span className="text-muted ml-2">by {e.actor}</span>}
      </div>
      {e.detail != null &&
       typeof e.detail === "object" &&
       Object.keys(e.detail as object).length > 0 && (
        <pre className="mt-1 text-[10px] text-muted whitespace-pre-wrap break-all">
          {JSON.stringify(e.detail, null, 2)}
        </pre>
      )}
    </div>
  );
}
