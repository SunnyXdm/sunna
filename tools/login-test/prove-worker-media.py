#!/usr/bin/env python3
"""Worker IPC proof under a non-root fixture account and a private Xvfb."""
import os
import socket
import struct
import subprocess
import sys
import time

assert os.geteuid() != 0 and os.environ.get("DISPLAY")
workers = []


def recv(channel, size):
    buf = bytearray()
    while len(buf) < size:
        chunk = channel.recv(size - len(buf))
        assert chunk, "worker disconnected"
        buf.extend(chunk)
    return bytes(buf)


def worker():
    picture, v = socket.socketpair()
    sound, a = socket.socketpair()
    env = {key: os.environ[key] for key in ("HOME", "PATH", "DISPLAY", "XAUTHORITY") if key in os.environ}
    env.update(XDG_SESSION_TYPE="x11", SUNNA_AUDIO_SOURCE="synthetic")
    p = subprocess.Popen([sys.argv[1], "--session-worker", "fixture"], env=env, stdin=v, stdout=a)
    v.close()
    a.close()
    sound.settimeout(5)
    picture.settimeout(5)
    workers.append((p, picture, sound))
    width, height = struct.unpack(">II", recv(picture, 8))
    assert width > 0 and height > 0
    return sound


def command(ch, op):
    ch.sendall(struct.pack(">BQ", op, 17))


def poll(ch):
    command(ch, 2)
    kind = recv(ch, 1)[0]
    if kind == 0:
        return None
    assert kind == 1
    size, = struct.unpack(">I", recv(ch, 4))
    assert size <= 16 * 1024 * 1024
    return recv(ch, size)


def copy(source, dest, text):
    command(source, 3)
    source.sendall(struct.pack(">BI", 1, len(text)) + text)
    for _ in range(40):
        if poll(dest) == text:
            return
        time.sleep(.05)
    raise AssertionError("clipboard transfer timed out")


try:
    first, second = worker(), worker()
    assert poll(first) is None and poll(second) is None
    command(first, 1)
    assert recv(first, 1) == bytes([1]) and any(recv(first, 1920))
    command(first, 4)
    copy(first, second, b"first local selection")
    copy(second, first, b"second local selection")
    command(first, 5)
    command(second, 5)
    print("PASS two non-root workers exchanged both X11 selections and returned a non-silent 10 ms stereo PCM frame")
finally:
    for p, picture, media in workers:
        picture.close()
        media.close()
        p.terminate()
        p.wait(timeout=5)
