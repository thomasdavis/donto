# Dev — working on the `donto-db` box

The `donto-db` VM in `apex-494316 / us-central1-a` is both the
production host for `genes.apexpots.com` **and** the primary development
environment. You SSH in, edit code, run `dev` for a hot-reload server,
or run `dep` to push the change to prod — all from the same machine.

See [PRODUCTION.md](PRODUCTION.md) for the topology and operational
details.

## SSH onto the box

From your laptop:

```bash
gcloud compute ssh donto-db --zone=us-central1-a --project=apex-494316
```

A shorter alias is set up in `~/.ssh/config` (see
[scripts/setup-laptop.sh](../scripts/setup-laptop.sh)):

```bash
ssh donto-db
```

For VS Code Remote-SSH: the same `donto-db` host alias works directly
once `gcloud compute config-ssh` has written its config.

## Workspace

```
$WORK = /mnt/donto-data/workspace/
├── donto                 ← this repo
├── donto-docs-internal   ← private PRDs / status reports (not a git repo)
├── dontopedia            ← Next.js debug UI + Node worker
├── genes                 ← genealogy research content (markdown, not deployed)
└── toiletpaper           ← Cloud Run app, deployed via apex CLI
```

Aliases (in `~/.bashrc` on the box):

```bash
work            # cd $WORK
donto           # cd $WORK/donto
dontopedia      # cd $WORK/dontopedia
toiletpaper     # cd $WORK/toiletpaper
genes-content   # cd $WORK/genes
psql-donto      # sudo docker exec -it donto-pg psql -U donto -d donto
dbsize          # show donto DB size
plogs <unit>    # follow journalctl for a systemd service
prestart <unit> # restart a systemd service
```

## `dev` — run the repo you're in, in dev mode

```bash
donto && dev          # cargo run dontosrv on :17879 (prod stays on :7879)
dontopedia && dev     # pnpm dev for apps/debug on :13002 (prod on :3002)
toiletpaper && dev    # pnpm dev on :13001
```

Dev ports never collide with the prod systemd-managed ports, so you
can have a hot-reload server up and prod live at the same time.

## `dep` — deploy the repo you're in

```bash
donto && dep                  # snapshot DB → cargo build → donto migrate
                              # → install dontosrv → restart all services
                              # → smoke test https://genes.apexpots.com/

donto && dep --skip-migrate   # skip the migration step
donto && dep --ref <git-sha>  # checkout a specific ref before building

dontopedia && dep             # pnpm install/build → restart donto-debug
toiletpaper && dep            # apex deploy . (requires apex CLI on box)
```

Every `dep` against `donto` snapshots the DB to
`/mnt/donto-data/backups/donto-pre-deploy-<UTC>.sql.gz` *before* any
migration. The old `dontosrv` binary is preserved as
`/usr/local/bin/dontosrv.bak-<UTC>` so a rollback is one `install` + one
`systemctl restart` away.

## Backups

- **Daily**: `/etc/cron.daily/donto-db-backup` writes
  `donto-daily-<UTC>.sql.gz` to `/mnt/donto-data/backups/`. 14-day
  retention.
- **Pre-deploy**: every `dep` against donto writes
  `donto-pre-deploy-<UTC>.sql.gz` — no automatic retention; clean up
  manually when you're confident the deploy stuck.

To restore a backup, see the restore snippet in
[PRODUCTION.md](PRODUCTION.md#backups).

## GitHub CLI

`gh` is installed on the box. Auth interactively once:

```bash
gh auth login        # pick HTTPS, paste a token or use device flow
gh auth status
```

Once authed, `gh` works from any of the repos under `$WORK`. The
hostname for git pushes will still be `github.com` over HTTPS using
the gh-managed credential helper — no SSH key management on the box.

## Cargo, pnpm, node, python

- Rust toolchain: `~/.cargo` (installed via rustup, sourced from
  `~/.bashrc`).
- Node 22 + pnpm via corepack (`corepack enable` already run).
- Python 3.12 system; `donto-api`'s requirements are installed
  system-wide. If you need a venv, create one inside the app dir.

The cargo target dir is `/mnt/donto-data/cargo-target` (not the default
`./target`) so it lives on the attached volume — faster incremental
builds and survives VM rebuilds. `dev` and `dep` both honor this.

## Editor

The shell + tmux + vim is enough for quick edits. For real work, use
**VS Code Remote-SSH** pointing at the `donto-db` host. The cargo
target dir is excluded by default `.vscode` settings in donto; if you
add other repos, exclude `node_modules` and `target` from search.

## Add a new repo to the box

```bash
work
git clone git@github.com:thomasdavis/<repo>.git
git config --global --add safe.directory $WORK/<repo>
```

If the repo has its own deploy mechanism, add a case to `~/bin/dep`
(or drop a `bin/deploy` script inside the repo — `dep` will pick it up
automatically).

## Common ops

```bash
# tail dontosrv logs
plogs dontosrv

# restart a single service
prestart donto-api

# inspect prod DB
psql-donto

# how big is the DB?
dbsize

# list recent backups
ls -lh /mnt/donto-data/backups/ | tail
```
