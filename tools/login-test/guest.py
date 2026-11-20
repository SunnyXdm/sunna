#!/usr/bin/env python3
"""Run a command through the fixture VM's private qemu-guest-agent channel."""
import base64
import fcntl
import json
import socket
import sys
import time

lock = open("/tmp/sunna-login-agent.lock", "w")
fcntl.flock(lock, fcntl.LOCK_EX)
sock = socket.socket(socket.AF_UNIX)
sock.settimeout(60)
sock.connect("/tmp/guest.sock")
stream = sock.makefile("rwb", buffering=0)


def call(name, **args):
    sock.sendall(json.dumps(dict(execute=name, arguments=args)).encode() + b"\n")
    result = json.loads(stream.readline())
    if "error" in result:
        raise RuntimeError(result["error"]["desc"])
    return result["return"]


# Recover message boundaries if an earlier guest-agent client disconnected.
stamp = time.monotonic_ns()
sock.sendall(b"\xff" + json.dumps(dict(execute="guest-sync-delimited", arguments=dict(id=stamp))).encode() + b"\n")
while True:
    line = stream.readline()
    if not line:
        sys.exit("guest agent closed the channel")
    if b"\xff" in line and json.loads(line.split(b"\xff")[-1]).get("return") == stamp:
        break

if len(sys.argv) == 4 and sys.argv[1] in ("put", "get"):
    put = sys.argv[1] == "put"
    local, remote = sys.argv[2:] if put else sys.argv[2:][::-1]
    handle = call("guest-file-open", path=remote, mode="wb" if put else "rb")
    try:
        with open(local, "rb" if put else "wb") as file:
            while True:
                if put:
                    chunk = file.read(256 * 1024)
                    if not chunk:
                        break
                    wrote = call("guest-file-write", handle=handle,
                                 **{"buf-b64": base64.b64encode(chunk).decode()})
                    if wrote["count"] != len(chunk):
                        raise OSError("short guest write")
                else:
                    chunk = call("guest-file-read", handle=handle, count=256 * 1024)
                    file.write(base64.b64decode(chunk.get("buf-b64", "")))
                    if chunk["eof"]:
                        break
    finally:
        call("guest-file-close", handle=handle)
    sys.exit(0)

if len(sys.argv) < 2:
    sys.exit("guest /absolute/path [args...]")
pid = call("guest-exec", path=sys.argv[1], arg=sys.argv[2:], **{"capture-output": True})["pid"]
while True:
    result = call("guest-exec-status", pid=pid)
    if result["exited"]:
        for key, dest in [("out-data", sys.stdout.buffer), ("err-data", sys.stderr.buffer)]:
            dest.write(base64.b64decode(result.get(key, "")))
        sys.exit(result.get("exitcode", 1))
    time.sleep(0.2)
