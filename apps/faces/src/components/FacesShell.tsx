"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import {
  donto,
  type DontoClient,
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

export function FacesShell({ dontosrvUrl }: Props) {
  const client = useMemo<DontoClient>(() => donto(dontosrvUrl), [dontosrvUrl]);
  const colorOf = useRef(makeColorMap()).current;

  const [subjects, setSubjects] = useState<SubjectsResponse["subjects"]>([]);
  const [subject, setSubject] = useState<string | null>(null);
  const [rows,    setRows]    = useState<Statement[]>([]);
  const [status,  setStatus]  = useState("loading subjects…");
  const [cursor,  setCursor]  = useState<CubePoint | null>(null);

  // Bootstrap the picker.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const r = await client.subjects();
        if (cancelled) return;
        // Promote ex:darnell-brooks if present.
        const ordered = r.subjects.slice().sort((a, b) => {
          if (a.subject === "ex:darnell-brooks") return -1;
          if (b.subject === "ex:darnell-brooks") return  1;
          return b.count - a.count;
        });
        setSubjects(ordered);
        if (ordered.length > 0) setSubject(ordered[0]!.subject);
        else setStatus("no subjects in donto yet — ingest a fixture first");
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setStatus(`cannot reach dontosrv at ${dontosrvUrl}: ${msg}`);
      }
    })();
    return () => { cancelled = true; };
  }, [client, dontosrvUrl]);

  // Load the selected subject's history.
  useEffect(() => {
    if (!subject) return;
    let cancelled = false;
    setStatus(`loading ${subject}…`);
    (async () => {
      try {
        const r = await client.history(subject);
        if (cancelled) return;
        setRows(r.rows);
        setStatus(
          r.rows.length === 0
            ? `no statements about ${subject}`
            : `${r.rows.length} statement${r.rows.length === 1 ? "" : "s"}`,
        );
        // Reset cursor to the median of the cube whenever the subject changes.
        if (r.rows.length) {
          const b = cubeBounds(r.rows);
          setCursor({
            valid: (b.vMin + b.vMax) / 2,
            tx:    Math.min(b.tMax, Date.now()),
          });
        } else {
          setCursor(null);
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setStatus(`load failed: ${msg}`);
      }
    })();
    return () => { cancelled = true; };
  }, [client, subject]);

  const bounds   = useMemo(() => cubeBounds(rows), [rows]);
  const contexts = useMemo(() => distinctContexts(rows), [rows]);

  return (
    <div className="h-screen flex flex-col">
      <header className="flex items-center gap-3 px-4 py-2 border-b border-rule bg-panel">
        <h1 className="text-accent text-sm tracking-wider m-0">donto · faces</h1>
        <select
          className="bg-paper border border-rule text-ink px-2 py-1 text-xs min-w-[280px]"
          value={subject ?? ""}
          onChange={(e) => setSubject(e.target.value)}
        >
          {subjects.map((s) => (
            <option key={s.subject} value={s.subject}>
              {s.subject} ({s.count})
            </option>
          ))}
        </select>
        <input
          className="bg-paper border border-rule text-ink px-2 py-1 text-xs min-w-[280px]"
          placeholder="or paste a subject IRI and press enter"
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              const v = (e.target as HTMLInputElement).value.trim();
              if (v) setSubject(v);
            }
          }}
        />
        <span className="text-muted text-xs ml-auto">{status}</span>
      </header>

      <main className="flex-1 grid grid-rows-2 min-h-0">
        <div className="grid grid-cols-2 min-h-0">
          <Panel
            title="stratigraph"
            legend="x: valid_time · y: tx_time (down = older belief)"
          >
            <Stratigraph rows={rows} bounds={bounds} colorOf={colorOf} />
          </Panel>
          <Panel
            title="rashomon hall"
            legend="lanes: contexts · red lines: same predicate, diverging objects"
          >
            <Rashomon rows={rows} bounds={bounds} contexts={contexts} colorOf={colorOf} />
          </Panel>
        </div>
        <div className="grid grid-cols-2 min-h-0">
          <Panel
            title="probe"
            legend="click anywhere · cursor = (valid_time, tx_time)"
          >
            <Probe
              rows={rows}
              bounds={bounds}
              cursor={cursor}
              onCursor={setCursor}
              colorOf={colorOf}
            />
          </Panel>
          <Panel
            title="row stream"
            legend="every statement, every polarity, no resolution"
          >
            <RowStream rows={rows} colorOf={colorOf} />
          </Panel>
        </div>
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
