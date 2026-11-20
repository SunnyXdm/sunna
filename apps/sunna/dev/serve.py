#!/usr/bin/env python3
"""Serve a folder for looking at the UI in a browser, never cached.

    serve.py PORT BIND DIRECTORY

Plain `python3 -m http.server` lets browsers keep old CSS and scripts across
a reload, so a fix can look like it didn't land.
"""
import functools
import http.server
import sys


class Handler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


port, bind, directory = int(sys.argv[1]), sys.argv[2], sys.argv[3]
http.server.ThreadingHTTPServer((bind, port), functools.partial(Handler, directory=directory)).serve_forever()
