#!/usr/bin/env bash
# Demo: the Lean kernel rejects a wrong theorem.
#
# Flips one theorem in lean/Donto/Theorems.lean to a false statement,
# runs `lake build`, captures the kernel error, restores, and rebuilds
# clean. Demonstrates that the donto spec is enforced by the Lean kernel
# on every commit — not just hoped for.
set -euo pipefail

FILE="lean/Donto/Theorems.lean"
BACKUP="${FILE}.bak"

step() { printf "\n\033[1;36m── %s ──\033[0m\n" "$*"; }

step "0. Baseline: lake build is green"
(cd lean && PATH=$HOME/.elan/bin:$PATH lake build) 2>&1 | tail -3

step "1. Break a theorem"
cp "$FILE" "$BACKUP"
trap 'mv "$BACKUP" "$FILE"' EXIT
# Replace `Truth.visibleByDefault s = true ↔ s.polarity = .asserted`
# with `... ↔ s.polarity = .negated` — a false statement.
sed -i 's|s.polarity = .asserted := by|s.polarity = .negated := by|' "$FILE"
echo "Modified line:"
grep -n 'polarity = .negated := by' "$FILE" || true

step "2. lake build — expect failure"
set +e
(cd lean && PATH=$HOME/.elan/bin:$PATH lake build) > /tmp/lake-fail.log 2>&1
RC=$?
set -e
echo "Exit code: $RC"
echo "Tail of lake output:"
tail -15 /tmp/lake-fail.log

if [[ $RC -eq 0 ]]; then
    echo "ERROR: build unexpectedly succeeded with broken theorem"
    exit 1
fi

step "3. Restore the theorem"
mv "$BACKUP" "$FILE"
trap - EXIT

step "4. lake build — green again"
(cd lean && PATH=$HOME/.elan/bin:$PATH lake build) 2>&1 | tail -3

echo
echo "Conclusion: a future PR that violates a documented donto invariant"
echo "fails CI before it can land."
