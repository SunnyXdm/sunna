#!/usr/bin/env python3
"""Sunna log collector.

Receives NDJSON batches from sunnad / sunna-cli (crates/telemetry) and appends
them to one file per process run:

    <dir>/<YYYY-MM-DD>/<device>/<role>-<run>.ndjson

Meant for a private network (bind it to a tailnet address), with a bearer
token as the only auth. Standard library only.

    SUNNA_LOG_TOKEN=... python3 sunna_logd.py --bind 100.x.y.z --port 48900 --dir ~/sunna-logs
"""

import argparse
import datetime
import hmac
import json
import os
import re
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MAX_BODY = 16 * 1024 * 1024
SAFE = re.compile(r"[^A-Za-z0-9._-]")


def safe(value, fallback):
    value = SAFE.sub("_", value or "")[:80]
    return value or fallback


class Handler(BaseHTTPRequestHandler):
    server_version = "sunna-logd/1"

    def log_message(self, fmt, *args):  # keep stdout for real events only
        pass

    def reply(self, code, body=b""):
        self.send_response(code)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/health":
            return self.reply(200, b"ok\n")
        self.reply(404)

    def do_POST(self):
        if self.path != "/ingest":
            return self.reply(404)
        auth = self.headers.get("Authorization", "")
        expected = "Bearer " + self.server.token
        if not hmac.compare_digest(auth.encode(), expected.encode()):
            return self.reply(401)
        length = int(self.headers.get("Content-Length") or 0)
        if length <= 0 or length > MAX_BODY:
            return self.reply(413)
        body = self.rfile.read(length)
        device = safe(self.headers.get("X-Sunna-Device"), "unknown-device")
        role = safe(self.headers.get("X-Sunna-Role"), "unknown-role")
        run = safe(self.headers.get("X-Sunna-Run"), "unknown-run")
        day = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d")
        folder = os.path.join(self.server.dir, day, device)
        os.makedirs(folder, exist_ok=True)
        path = os.path.join(folder, f"{role}-{run}.ndjson")
        received = datetime.datetime.now(datetime.timezone.utc).isoformat()
        lines = 0
        with open(path, "a", encoding="utf-8") as out:
            for raw in body.decode("utf-8", "replace").splitlines():
                if not raw.strip():
                    continue
                try:
                    record = json.loads(raw)
                except json.JSONDecodeError:
                    record = {"kind": "unparsed", "raw": raw[:2000]}
                record["rx"] = received
                out.write(json.dumps(record, separators=(",", ":")) + "\n")
                lines += 1
        if lines and body.count(b'"kind":"env"'):
            print(f"{received} new run: {device} {role} {run}", flush=True)
        self.reply(204)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bind", required=True, help="address to bind (use the tailnet IP)")
    parser.add_argument("--port", type=int, default=48900)
    parser.add_argument("--dir", default=os.path.expanduser("~/sunna-logs"))
    args = parser.parse_args()
    token = os.environ.get("SUNNA_LOG_TOKEN", "")
    if len(token) < 16:
        sys.exit("set SUNNA_LOG_TOKEN (16+ chars)")
    server = ThreadingHTTPServer((args.bind, args.port), Handler)
    server.token = token
    server.dir = args.dir
    os.makedirs(args.dir, exist_ok=True)
    print(f"sunna-logd listening on {args.bind}:{args.port}, writing {args.dir}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
