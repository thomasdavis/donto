/** Write-side HTTP surface: /contexts/ensure, /assert, /assert/batch, /retract. */
import { donto, type Literal, type Polarity } from "./index";

export interface EnsureContextInput {
  iri: string;
  kind?: "source" | "hypothesis" | "derived" | "snapshot" | "user" | string;
  mode?: "permissive" | "strict" | string;
  parent?: string | null;
}

export interface AssertInput {
  subject: string;
  predicate: string;
  /** Exactly one of object_iri or object_lit. */
  object_iri?: string | null;
  object_lit?: Literal | null;
  context?: string;
  polarity?: Polarity;
  maturity?: number;
  valid_from?: string | null;
  valid_to?: string | null;
}

async function postJson<T>(baseUrl: string, path: string, body: unknown): Promise<T> {
  const trimmed = donto(baseUrl).baseUrl;
  const r = await fetch(`${trimmed}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!r.ok) throw new Error(`dontosrv ${path}: ${r.status} ${r.statusText}`);
  return (await r.json()) as T;
}

export async function ensureContext(
  baseUrl: string,
  input: EnsureContextInput,
): Promise<{ iri: string; ok: boolean }> {
  return postJson(baseUrl, "/contexts/ensure", input);
}

export async function assert(
  baseUrl: string,
  input: AssertInput,
): Promise<{ statement_id: string }> {
  return postJson(baseUrl, "/assert", input);
}

export async function assertBatch(
  baseUrl: string,
  statements: AssertInput[],
): Promise<{ inserted: number }> {
  return postJson(baseUrl, "/assert/batch", { statements });
}

export async function retract(
  baseUrl: string,
  statementId: string,
): Promise<{ statement_id: string; retracted: boolean }> {
  return postJson(baseUrl, "/retract", { statement_id: statementId });
}
