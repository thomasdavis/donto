#!/usr/bin/env bash
# scripts/setup-laptop.sh — one-time laptop-side setup for working
# against the donto-db dev box.
#
# Adds an `ssh donto-db` alias by running `gcloud compute config-ssh`
# and verifies you can reach the box.

set -euo pipefail

PROJECT="apex-494316"
ZONE="us-central1-a"
INSTANCE="donto-db"

if ! command -v gcloud >/dev/null; then
  echo "gcloud not installed. Install: https://cloud.google.com/sdk/docs/install" >&2
  exit 1
fi

ACTIVE=$(gcloud config get-value account 2>/dev/null || true)
echo "▸ gcloud account: $ACTIVE"
echo "▸ gcloud project: $(gcloud config get-value project 2>/dev/null)"

if [[ "$ACTIVE" != "thomasalwyndavis@gmail.com" ]]; then
  echo "✗ expected thomasalwyndavis@gmail.com active; switch with:"
  echo "    gcloud config set account thomasalwyndavis@gmail.com"
  exit 1
fi

if [[ "$(gcloud config get-value project 2>/dev/null)" != "$PROJECT" ]]; then
  echo "▸ setting project to $PROJECT"
  gcloud config set project "$PROJECT" >/dev/null
fi

echo "▸ writing ~/.ssh/config entries for all GCE instances in $PROJECT"
gcloud compute config-ssh --quiet >/dev/null

# `gcloud compute config-ssh` writes a working Host entry named
# "<instance>.<zone>.<project>" with HostName=<IP>. We add a short alias
# "donto-db" pointing at the same IP. Re-run this script after VM
# restart if the NAT IP changes (it pins the IP).

SHORT="donto-db"
LONG="${INSTANCE}.${ZONE}.${PROJECT}"
SSHCFG="$HOME/.ssh/config"

IP=$(gcloud compute instances describe "$INSTANCE" \
      --zone="$ZONE" --project="$PROJECT" \
      --format='value(networkInterfaces[0].accessConfigs[0].natIP)')
# Pull the HostKeyAlias from the gcloud-generated long-name block.
# gcloud uses "Key=Value" or "Key Value"; normalise both.
HOSTKEY_ALIAS=$(awk -v h="Host $LONG" '
  $0 == h {found=1; next}
  found && /^Host / {exit}
  found {
    line=$0; gsub(/=/, " ", line)
    n=split(line, f); if (n >= 2 && f[1] == "HostKeyAlias") {print f[2]; exit}
  }
' "$SSHCFG")
# gcloud config-ssh doesn't pin a User — it uses the calling shell's user.
USERNAME=$(whoami)

# Strip any prior `Host donto-db` block we may have written (idempotent).
if grep -q "^Host $SHORT\$" "$SSHCFG" 2>/dev/null; then
  cp "$SSHCFG" "$SSHCFG.bak.$(date +%s)"
  awk -v h="Host $SHORT" '
    $0 == h {skip=1; next}
    skip && /^Host / {skip=0}
    !skip {print}
  ' "$SSHCFG.bak.$(date +%s)" > "$SSHCFG"
  echo "  removed stale Host $SHORT block (backup made)"
fi

{
  echo
  echo "Host $SHORT"
  echo "  HostName $IP"
  echo "  User $USERNAME"
  echo "  IdentityFile ~/.ssh/google_compute_engine"
  echo "  UserKnownHostsFile ~/.ssh/google_compute_known_hosts"
  [[ -n "$HOSTKEY_ALIAS" ]] && echo "  HostKeyAlias $HOSTKEY_ALIAS"
  echo "  IdentitiesOnly yes"
  echo "  CheckHostIP no"
} >> "$SSHCFG"
echo "  wrote 'Host $SHORT' (IP=$IP, User=$USERNAME) to $SSHCFG"

echo "▸ smoke test: ssh donto-db 'hostname'"
ssh -o ConnectTimeout=8 -o BatchMode=yes "$SHORT" hostname || {
  echo "✗ failed; fall back to: gcloud compute ssh donto-db --zone=$ZONE --project=$PROJECT"
  exit 1
}

echo "✓ ready. Use 'ssh donto-db' from now on, or VS Code Remote-SSH host '$SHORT'."
