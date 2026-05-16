from mitmproxy import http
import json
import os
from datetime import datetime, timezone


OUTFILE = os.environ.get("NETWORK_JAIL_HOST_LOG", "/data/observed-hosts.jsonl")
ALLOWLIST_FILE = os.environ.get(
    "NETWORK_JAIL_ALLOWLIST_FILE", "/addons/allowlist-run.txt"
)
ENFORCE_ALLOWLIST = os.environ.get("NETWORK_JAIL_ENFORCE_ALLOWLIST", "1") != "0"
PHASE = os.environ.get("NETWORK_JAIL_PHASE", "unknown")


def _load_allowlist():
    allowed = set()
    if not os.path.exists(ALLOWLIST_FILE):
        return allowed
    with open(ALLOWLIST_FILE, "r", encoding="utf-8") as fh:
        for raw in fh:
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            allowed.add(line.lower())
    return allowed


def _host_matches(token: str, entry: str) -> bool:
    # Wildcard suffix support: "*.huggingface.co" matches "cdn-lfs.huggingface.co"
    if entry.startswith("*."):
        return token == entry[2:] or token.endswith(entry[1:])
    return token == entry


def _is_allowed(host: str, port: int) -> bool:
    token_host = host.lower()
    token_host_port = f"{token_host}:{port}"
    for entry in _ALLOWED:
        if _host_matches(token_host, entry) or _host_matches(token_host_port, entry):
            return True
    return False


def _write(record):
    os.makedirs(os.path.dirname(OUTFILE), exist_ok=True)
    with open(OUTFILE, "a", encoding="utf-8") as fh:
        fh.write(json.dumps(record, sort_keys=True) + "\n")


_ALLOWED = _load_allowlist()


def request(flow: http.HTTPFlow) -> None:
    req = flow.request
    record = {
        "ts": datetime.now(timezone.utc).isoformat(),
        "phase": PHASE,
        "kind": "http_request",
        "scheme": req.scheme,
        "host": req.host,
        "port": req.port,
        "method": req.method,
        "path": req.path,
        "pretty_url": req.pretty_url,
    }

    if ENFORCE_ALLOWLIST and not _is_allowed(req.host, req.port):
        record["kind"] = "http_request_blocked"
        record["blocked_reason"] = "egress_host_not_allowlisted"
        flow.response = http.Response.make(
            403,
            b"network-jail: outbound blocked by allowlist policy\n",
            {"Content-Type": "text/plain"},
        )

    _write(record)
