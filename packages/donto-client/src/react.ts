/** Reactions: endorse / reject / cite / supersede a statement. */
import { donto } from "./index";

export type ReactionKind = "endorses" | "rejects" | "cites" | "supersedes";

export interface ReactInput {
  source: string; // statement_id uuid
  kind: ReactionKind;
  object_iri?: string | null;
  context?: string;
  actor?: string | null;
}

export interface ReactionOut {
  reaction_id: string;
  kind: ReactionKind;
  object_iri: string | null;
  context: string;
  polarity: "asserted" | "negated" | "absent" | "unknown";
}

export interface ReactionsResponse {
  reactions: ReactionOut[];
  counts: {
    endorses: number;
    rejects: number;
    cites: number;
    supersedes: number;
  };
}

export async function react(
  baseUrl: string,
  input: ReactInput,
): Promise<{ reaction_id: string }> {
  const base = donto(baseUrl).baseUrl;
  const r = await fetch(`${base}/react`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(input),
  });
  if (!r.ok) throw new Error(`dontosrv /react: ${r.status}`);
  return (await r.json()) as { reaction_id: string };
}

export async function reactionsFor(
  baseUrl: string,
  statementId: string,
): Promise<ReactionsResponse> {
  const base = donto(baseUrl).baseUrl;
  const r = await fetch(
    `${base}/reactions/${encodeURIComponent(statementId)}`,
  );
  if (!r.ok) throw new Error(`dontosrv /reactions: ${r.status}`);
  return (await r.json()) as ReactionsResponse;
}
