# @donto/faces

Three-lens visualisation of donto's bitemporal cube. Next.js 15 + React 19,
talks to a running `dontosrv` over HTTP.

## Lenses

- **Stratigraph** — geological cross-section. x = `valid_time`,
  y = `tx_time` (down = older belief). Each statement is a coloured stratum;
  retractions appear with a red border and lower opacity.
- **Rashomon Hall** — every context gets a horizontal lane. Same predicate
  + same `valid_time` but diverging objects across two lanes → red line.
  The eye finds the disagreements automatically.
- **Probe** — a 2D plane (valid × tx). Click anywhere; the side panel lists
  every statement live at that exact (valid_time, tx_time) coordinate.
- **Row Stream** — the literal output, terminal-style, no resolution.

## Run

From the monorepo root:

```bash
just pg-up
just ingest-brooks      # seeds the demo subject ex:darnell-brooks
just faces              # starts dontosrv (:7878) and Next.js (:3000)
```

Then open <http://localhost:3000>.

## Configuration

- `DONTOSRV_URL` — base URL of dontosrv. Defaults to
  `http://localhost:7878`. Read at build time and exposed to the client
  bundle as `NEXT_PUBLIC_DONTOSRV_URL`.

## Stack

- Next.js 15 (app router, Turbopack dev, React 19)
- Tailwind CSS 3
- Plain SVG components (no charting library) — the geometry is the message.
- `@donto/client` (workspace package) for typed access to dontosrv.
