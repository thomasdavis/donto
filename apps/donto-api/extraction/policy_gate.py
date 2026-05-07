"""Policy gate before external model calls.

PRD M5 acceptance:
  > Policy blocks external calls for restricted sources.

`source_permits_external_model(authoriser, source_iri, action)`
asks the Trust Kernel whether the *anonymous* caller has the
requested action on the source. If denied, the orchestrator must
not call the external model and should record the refusal.

The `authoriser` is a callable
  (caller, target_kind, target_id, action) -> bool
so this module doesn't pin dontosrv's HTTP shape and stays
testable with a fake.
"""

from __future__ import annotations

from typing import Callable

Authoriser = Callable[[str, str, str, str], bool]

# "agent:anonymous" is the Trust Kernel's catch-all caller used when
# no caller header is present. Probing as anonymous gives us the most
# restrictive answer — the right gate for "should we send this text
# to a third-party model?".
_PROBE_CALLER = "agent:anonymous"


def source_permits_external_model(
    authoriser: Authoriser,
    source_iri: str,
    action: str = "derive",
) -> bool:
    """Return True iff `action` is allowed on the source for the
    anonymous caller. Default action is `derive` (the action that
    covers "send to an external model and consume its output")."""
    return bool(authoriser(_PROBE_CALLER, "source", source_iri, action))
