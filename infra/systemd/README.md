# donto systemd units

Reference copies of the systemd units running on the production
donto-db VM. Install with:

```bash
sudo cp donto-analyze.service /etc/systemd/system/
sudo cp donto-analyze.timer   /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now donto-analyze.timer
```

## Units

| Unit | Purpose | Trigger |
|------|---------|---------|
| `donto-analyze.service` | One-shot. Runs `donto analyze paraconsistency` then `donto analyze reviewer-acceptance` against the prod DB. Findings land in `donto_detector_finding`. | Triggered by `donto-analyze.timer`. |
| `donto-analyze.timer` | Nightly. Fires the analyze service at 03:30 UTC. | `OnCalendar=*-*-* 03:30:00 UTC` |

## Not (yet) in this directory

The four long-running services on the box have their unit files
in `/etc/systemd/system/` but are not yet mirrored here:

- `dontosrv.service` (axum HTTP sidecar, :7879)
- `donto-api.service` (FastAPI public API, :8000)
- `donto-api-worker.service` (Temporal worker)
- `donto-debug.service` (Next.js debug UI, :3002)

A future migration would copy those in and switch the box to install
from this directory.
