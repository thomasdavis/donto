# Production — `genes.apexpots.com`

The genealogy product built on donto runs as `genes.apexpots.com`,
fronted by Cloudflare and served from a single GCE VM in `apex-494316`.
This doc is the source of truth for operating it. If anything below
disagrees with reality, fix reality or fix this file — don't let them
drift.

> donto is a work-in-progress database. Migrations are **not** required
> to be safe to re-run on populated data. Always snapshot before
> applying.

## Topology

```
                                   ┌──────────────────────────────┐
       Cloudflare ─ proxied DNS ───►        Caddy 80/443           │
       (DNSSEC, edge cache, TLS)   │  (auto-cert via Let's Encrypt)│
                                   ├──────────────────────────────┤
                                   │  genes.apexpots.com           │
                                   │   ├─ /firehose/stream → :8000 │  donto-api (Python)
                                   │   ├─ /firehose       → :3002 │  donto-debug (Next.js)
                                   │   ├─ /queue, /report,        │
                                   │   │  /pulse, /predicates,    │
                                   │   │  /explore/*, /_next/*    → :3002 │
                                   │   └─ everything else → :8000 │
                                   │                              │
                                   │  debug.genes.apexpots.com    │
                                   │   └─ all → :3002             │
                                   │                              │
                                   │  trees.apexpots.com          │
                                   │   └─ all → :5055 (gramps-web,│
                                   │     a separate genealogy app)│
                                   └──────────────────────────────┘
                                                ▲
                                                │ systemd-managed
                                                │
                          ┌─────────────────────┴─────────────────────┐
                          │ VM: donto-db (us-central1-a, e2-standard) │
                          │ NAT 136.114.118.108, internal 10.128.0.3  │
                          ├───────────────────────────────────────────┤
                          │ /usr/local/bin/dontosrv      :7879  Rust  │  ← THE donto code
                          │ donto-api.service            :8000  uvicorn (apps/donto-api, Python)
                          │ donto-api-worker.service     n/a    Temporal worker (Python)
                          │ donto-debug.service          :3002  Next.js (dontopedia/apps/debug)
                          │                                           │
                          │ docker: donto-pg             :5432  postgres:16
                          │   data: /mnt/donto-data/pgdata (bind)     │
                          │ docker: dontopedia-worker    n/a    Node worker (Temporal)
                          │ docker: gramps-web           :5055  unrelated
                          │ docker: temporal+temporal-ui :7233/:8233  │
                          └───────────────────────────────────────────┘
```

## Repos

- **donto** (this repo) — `dontosrv` Rust binary + SQL migrations + `donto-cli` (used by deploy).
- **dontopedia** — Next.js debug UI + Node worker. Cloned to `/mnt/donto-data/workspace/dontopedia` on the box.

Both repos live under `/mnt/donto-data/workspace/` on the VM. The
`/mnt/donto-data` mount is the **attached persistent disk** — that's
also where `pgdata` and database backups live, so a VM rebuild does
not destroy data.

## Backups

- Location: `/mnt/donto-data/backups/` on the VM.
- Format: `pg_dump --format=plain` piped through gzip.
- Filename: `donto-pre-<event>-<UTC timestamp>.sql.gz`.
- Backups are made automatically by `scripts/deploy-genes.sh` before
  any migration step. No remote/off-VM copy yet — see TODO below.

To restore from a backup:

```bash
gcloud compute ssh donto-db --zone=us-central1-a --project=apex-494316
sudo docker exec -i donto-pg dropdb -U donto --if-exists donto
sudo docker exec -i donto-pg createdb -U donto donto
gunzip -c /mnt/donto-data/backups/donto-pre-migrate-<TS>.sql.gz \
  | sudo docker exec -i donto-pg psql -U donto -d donto
```

## Secrets

- `donto-pg` password: in the container's `POSTGRES_PASSWORD` env, also
  embedded in `dontosrv.service` and `dontopedia-worker` containers as
  `DONTO_DSN=postgres://donto:<pw>@127.0.0.1:5432/donto`.
- Cloud Run `dontopedia-*` services use Secret Manager (`DONTO_DSN`).
  Keep both in sync if the password rotates.

## Cloud Run mirrors

Some donto-adjacent services *also* exist on Cloud Run (`dontosrv`,
`dontopedia-web`, `dontopedia-worker`, `dontopedia-agent-runner`).
They are an older deployment path — `genes.apexpots.com` does **not**
route to them; everything terminates at the VM's Caddy. Treat the
Cloud Run revisions as a fallback; updating them is out of scope for
the `deploy-genes.sh` script.

## Deploy

Use `scripts/deploy-genes.sh` from a checkout of this repo on your
laptop. It will:

1. Snapshot `/mnt/donto-data/backups/donto-pre-deploy-<TS>.sql.gz`.
2. Sync the donto repo on the VM to a target git ref (default
   `origin/main`).
3. `cargo build --release -p dontosrv -p donto-cli` on the VM.
4. Run `donto migrate` against `donto-pg`.
5. Install the new `dontosrv` binary to `/usr/local/bin/dontosrv`,
   backing up the old one to `/usr/local/bin/dontosrv.bak-<TS>`.
6. `systemctl restart dontosrv donto-api donto-api-worker donto-debug`.
7. Curl smoke-test `https://genes.apexpots.com/`.

```bash
./scripts/deploy-genes.sh                  # deploys origin/main
./scripts/deploy-genes.sh --ref <sha>      # specific ref
./scripts/deploy-genes.sh --skip-migrate   # binaries + restart only
./scripts/deploy-genes.sh --dry-run        # print plan, do nothing
```

The first run on a fresh VM requires the Rust toolchain to be present
on the box (`scripts/bootstrap-genes-box.sh`, TODO).

## Routine ops

```bash
# tail dontosrv logs
gcloud compute ssh donto-db --zone=us-central1-a --project=apex-494316 \
  -- "sudo journalctl -u dontosrv -f"

# restart everything (does not migrate)
gcloud compute ssh donto-db --zone=us-central1-a --project=apex-494316 \
  -- "sudo systemctl restart dontosrv donto-api donto-api-worker donto-debug"

# psql into prod (read-only-ish — be careful)
gcloud compute ssh donto-db --zone=us-central1-a --project=apex-494316 \
  -- "sudo docker exec -it donto-pg psql -U donto -d donto"
```

## TODO

- Move backups to GCS so a VM disk loss is recoverable. Daily cron +
  retention.
- Add a `bootstrap-genes-box.sh` that installs Rust, build deps, Caddy,
  Docker, and the systemd unit files — so the VM is reproducible.
- Publish `donto-cli`, `dontosrv`, and `pg_donto` as proper crates /
  release artifacts so the deploy can `cargo install` a pinned version
  instead of building from a working copy on the box.
- Cut the Cloud Run mirror services if they're not load-bearing.
