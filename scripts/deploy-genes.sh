#!/usr/bin/env bash
# scripts/deploy-genes.sh — deploy donto to the genes.apexpots.com VM.
#
# What this does:
#   1. pg_dump the prod donto DB to /mnt/donto-data/backups/ on the VM.
#   2. git fetch + checkout target ref in /mnt/donto-data/workspace/donto.
#   3. cargo build --release -p dontosrv -p donto-cli on the VM.
#   4. donto migrate against the in-VM postgres.
#   5. Install the new dontosrv binary, backup the old one.
#   6. systemctl restart dontosrv + donto-api{,-worker} + donto-debug.
#   7. Smoke-test https://genes.apexpots.com/.
#
# Why "on the VM" instead of building locally:
#   The VM is x86_64 Linux, dev laptops are arm64 macOS. Cross-compiling
#   is possible but adds toolchain weight; building on the VM keeps the
#   deploy hermetic to the target and reuses the cargo cache at
#   /mnt/donto-data/cargo-target.
#
# See docs/PRODUCTION.md for the topology.

set -euo pipefail

PROJECT="apex-494316"
ZONE="us-central1-a"
INSTANCE="donto-db"
REPO_ON_VM="/mnt/donto-data/workspace/donto"
CARGO_TARGET="/mnt/donto-data/cargo-target"
BACKUP_DIR="/mnt/donto-data/backups"
SMOKE_URL="https://genes.apexpots.com/"

REF="origin/main"
SKIP_MIGRATE=0
SKIP_BUILD=0
DRY_RUN=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --ref)           REF="$2"; shift 2 ;;
    --skip-migrate)  SKIP_MIGRATE=1; shift ;;
    --skip-build)    SKIP_BUILD=1; shift ;;
    --dry-run)       DRY_RUN=1; shift ;;
    -h|--help)
      sed -n '2,20p' "$0"; exit 0 ;;
    *)
      echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

say() { printf '\033[1;36m▸ %s\033[0m\n' "$*"; }
ok()  { printf '\033[1;32m✓ %s\033[0m\n' "$*"; }
die() { printf '\033[1;31m✗ %s\033[0m\n' "$*" >&2; exit 1; }

ssh_box() {
  gcloud compute ssh "$INSTANCE" --zone="$ZONE" --project="$PROJECT" \
    --quiet --command="$1"
}

say "target VM:   $INSTANCE ($ZONE / $PROJECT)"
say "target ref:  $REF"
say "skip build:  $SKIP_BUILD     skip migrate: $SKIP_MIGRATE"
[[ $DRY_RUN == 1 ]] && { say "dry-run — nothing will be changed."; exit 0; }

# --- step 1: snapshot ---------------------------------------------------------
TS=$(date -u +%Y%m%dT%H%M%SZ)
say "1/7  pg_dump → $BACKUP_DIR/donto-pre-deploy-${TS}.sql.gz"
ssh_box "set -e
  sudo mkdir -p $BACKUP_DIR && sudo chown -R \$USER:\$USER $BACKUP_DIR
  sudo docker exec donto-pg pg_dump -U donto -d donto \
    --format=plain --no-owner --no-privileges \
    | gzip > $BACKUP_DIR/donto-pre-deploy-${TS}.sql.gz
  ls -lh $BACKUP_DIR/donto-pre-deploy-${TS}.sql.gz"
ok "backup taken"

# --- step 2: sync source ------------------------------------------------------
say "2/7  syncing $REPO_ON_VM to $REF"
ssh_box "set -e
  cd $REPO_ON_VM
  sudo git config --global --add safe.directory $REPO_ON_VM 2>/dev/null || true
  # stash any local changes — we never destroy them silently
  if ! sudo git diff --quiet || ! sudo git diff --cached --quiet || sudo test -n \"\$(sudo git ls-files --others --exclude-standard)\"; then
    sudo git stash push -u -m \"deploy-${TS}\" || true
  fi
  sudo git fetch --quiet origin
  sudo git checkout --quiet --detach $REF
  sudo git log --oneline -1"
ok "source synced"

# --- step 3: build ------------------------------------------------------------
if [[ $SKIP_BUILD == 0 ]]; then
  say "3/7  cargo build --release -p dontosrv -p donto-cli  (on VM)"
  ssh_box "set -e
    source \$HOME/.cargo/env
    cd $REPO_ON_VM
    sudo -E env PATH=\$PATH CARGO_TARGET_DIR=$CARGO_TARGET \
      cargo build --release -p dontosrv -p donto-cli
    ls -lh $CARGO_TARGET/release/dontosrv $CARGO_TARGET/release/donto"
  ok "binaries built"
else
  say "3/7  skipped (--skip-build)"
fi

# --- step 4: migrate ----------------------------------------------------------
if [[ $SKIP_MIGRATE == 0 ]]; then
  say "4/7  donto migrate"
  ssh_box "set -e
    cd $REPO_ON_VM
    sudo DONTO_DSN='postgres://donto:ier4wG7oETKMmXY16meC0TAv@127.0.0.1:5432/donto' \
      $CARGO_TARGET/release/donto migrate"
  ok "migrations applied"
else
  say "4/7  skipped (--skip-migrate)"
fi

# --- step 5: install dontosrv binary -----------------------------------------
say "5/7  installing /usr/local/bin/dontosrv (backup → .bak-${TS})"
ssh_box "set -e
  sudo cp /usr/local/bin/dontosrv /usr/local/bin/dontosrv.bak-${TS}
  sudo install -m 0755 $CARGO_TARGET/release/dontosrv /usr/local/bin/dontosrv
  ls -lh /usr/local/bin/dontosrv /usr/local/bin/dontosrv.bak-${TS}"
ok "binary installed"

# --- step 6: restart services -------------------------------------------------
say "6/7  systemctl restart dontosrv donto-api donto-api-worker donto-debug"
ssh_box "set -e
  sudo systemctl restart dontosrv
  sudo systemctl restart donto-api donto-api-worker donto-debug
  sleep 2
  sudo systemctl is-active dontosrv donto-api donto-api-worker donto-debug"
ok "services restarted"

# --- step 7: smoke ------------------------------------------------------------
say "7/7  smoke test $SMOKE_URL"
HTTP=$(curl -sSL -o /dev/null -w '%{http_code}' --max-time 15 "$SMOKE_URL" || echo "000")
if [[ "$HTTP" =~ ^2 ]] || [[ "$HTTP" =~ ^3 ]]; then
  ok "smoke OK ($HTTP)"
else
  die "smoke FAILED ($HTTP) — check journalctl on the box, consider rollback via dontosrv.bak-${TS}"
fi

echo
ok "deploy complete @ $TS"
