/** Shape validation client. */
import { donto } from "./index.js";

export interface Violation {
  focus:    string;
  reason:   string;
  evidence: string[];
}

export interface ShapeReport {
  shape_iri:   string;
  focus_count: number;
  violations:  Violation[];
}

export interface ValidateResponse {
  shape_iri: string;
  source:    "builtin" | "lean" | "cached";
  report?:   { focus_count: number; violations: Violation[] };
  /** Inline (non-cached) responses include violations directly. */
  focus_count?: number;
  violations?:  Violation[];
}

export async function validate(
  baseUrl: string,
  shapeIri: string,
  scope: { include: string[]; exclude?: string[] },
): Promise<ValidateResponse> {
  const r = await fetch(`${donto(baseUrl).baseUrl}/shapes/validate`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ shape_iri: shapeIri, scope }),
  });
  if (!r.ok) throw new Error(`shape validate: ${r.status}`);
  return (await r.json()) as ValidateResponse;
}
