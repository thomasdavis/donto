/** Stable per-context colour assignment. */
const PALETTE = [
  "#f0c674", "#82a45a", "#7aa2c0", "#c08bd5",
  "#d49a6a", "#a8a8a8", "#b87a7a", "#7ac0a8",
];

export function makeColorMap(): (ctx: string) => string {
  const seen = new Map<string, string>();
  return (ctx: string) => {
    if (!seen.has(ctx)) {
      seen.set(ctx, PALETTE[seen.size % PALETTE.length]!);
    }
    return seen.get(ctx)!;
  };
}

/** Format an ms-since-epoch as YYYY-MM-DD. */
export function fmtDate(ms: number | null | undefined): string {
  if (ms == null || !isFinite(ms)) return "∞";
  const d = new Date(ms);
  return (
    d.getUTCFullYear() +
    "-" +
    String(d.getUTCMonth() + 1).padStart(2, "0") +
    "-" +
    String(d.getUTCDate()).padStart(2, "0")
  );
}
