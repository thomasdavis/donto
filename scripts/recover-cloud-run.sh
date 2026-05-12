#!/usr/bin/env bash
# scripts/recover-cloud-run.sh
#
# Recovery script after a Cloud Run wipe (2026-05-13). Recreates every
# service that was deleted when `gcloud services disable run.googleapis.com
# --force` was run, restores the custom domain mappings, and applies the
# new policy: max-instances=1 across the board.
#
# Pre-requisite: billing must be restored on apex-494316. Until then
# every create call will fail with BILLING_DISABLED.
#
# This script DOES NOT restore Cloud Run versions of `orbis-*` — those
# were intentionally killed and should stay killed.
#
# Re-establishing secret env vars: services that previously read secrets
# (DONTO_DSN, etc.) need them re-wired with `--set-secrets`. The lines
# below preserve the bindings observed before the wipe.

set -euo pipefail

PROJECT="apex-494316"
REGION="us-central1"
REGISTRY="us-central1-docker.pkg.dev/${PROJECT}/apex-platform"

say() { printf '\033[1;36m▸ %s\033[0m\n' "$*"; }
ok()  { printf '\033[1;32m✓ %s\033[0m\n' "$*"; }
warn(){ printf '\033[1;33m! %s\033[0m\n' "$*"; }

deploy() {
  local name="$1"; shift
  say "deploy $name"
  gcloud run deploy "$name" \
    --image="${REGISTRY}/${name}:latest" \
    --project="$PROJECT" --region="$REGION" \
    --allow-unauthenticated \
    --quiet \
    "$@" || warn "$name failed — likely missing image or env"
}

# --- core ---------------------------------------------------------------
deploy apex-apiserver --memory=256Mi --cpu=1 --min-instances=0 --max-instances=1

# --- donto-adjacent (Cloud Run mirrors of services that also run on the VM)
# These are not on the live genes.apexpots.com path; keep them as a fallback.
deploy dontosrv \
  --memory=512Mi --cpu=1 --min-instances=0 --max-instances=1 \
  --add-cloudsql-instances=${PROJECT}:${REGION}:apex-postgres-staging \
  --set-secrets=DONTO_DSN=dontosrv-dsn:latest

deploy dontopedia-web \
  --memory=1Gi --cpu=1 --min-instances=0 --max-instances=1 \
  --set-env-vars=DONTOSRV_URL=http://136.114.118.108:7879,NEXT_PUBLIC_DONTOSRV_URL=http://136.114.118.108:7879 \
  --set-secrets=DONTO_DSN=donto-dsn:latest

deploy dontopedia-worker \
  --memory=1Gi --cpu=1 --min-instances=0 --max-instances=1 \
  --set-env-vars=DONTOSRV_URL=https://dontosrv-587706120371.us-central1.run.app \
  --set-secrets=DONTO_DSN=donto-dsn:latest

deploy dontopedia-agent-runner \
  --memory=512Mi --cpu=1 --min-instances=0 --max-instances=1

# --- ajaxdavis.dev (your personal site) ---------------------------------
deploy lordajax --memory=512Mi --cpu=1 --min-instances=0 --max-instances=1

# --- toiletpaper -------------------------------------------------------
# Easier: re-deploy via apex CLI from the repo so apex.toml is the source
# of truth (DATABASE_URL, OPENROUTER_API_KEY, CLAUDE_CREDENTIALS, etc.).
warn "toiletpaper-web: prefer 'apex deploy .' from the toiletpaper repo (apex.toml has the right env)"
# Fallback bare redeploy without env (will start but won't work properly):
deploy toiletpaper-web --memory=512Mi --cpu=1 --min-instances=0 --max-instances=1

# --- tpmjs.com ---------------------------------------------------------
deploy tpmjs-web --memory=2Gi --cpu=1 --min-instances=0 --max-instances=1

# --- misc small ones ---------------------------------------------------
deploy apex-hello --memory=256Mi --cpu=1 --min-instances=0 --max-instances=1
deploy blahworld   --memory=256Mi --cpu=1 --min-instances=0 --max-instances=1
deploy faces       --memory=256Mi --cpu=1 --min-instances=0 --max-instances=1

# --- temporal (Cloud Run instance) -------------------------------------
# This was min=1 previously. Keeping max=1 per user policy.
deploy temporal --memory=1Gi --cpu=1 --min-instances=0 --max-instances=1

# --- domain mappings ---------------------------------------------------
say "re-establishing domain mappings"
map() {
  local domain="$1" svc="$2"
  gcloud beta run domain-mappings create \
    --service="$svc" --domain="$domain" \
    --project="$PROJECT" --region="$REGION" --quiet 2>&1 \
    | grep -v "already exists" || true
}
map ajaxdavis.dev      lordajax
map www.ajaxdavis.dev  lordajax
# orbis mappings intentionally NOT restored.

ok "recovery attempt complete — check 'gcloud run services list --region=$REGION'"
