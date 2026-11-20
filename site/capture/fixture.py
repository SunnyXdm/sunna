"""Serve the real web UI with neutral sample data on the capture loopback port."""
import functools
import re
import http.server
from pathlib import Path
from io import BytesIO

class Fixture(http.server.SimpleHTTPRequestHandler):
    def send_head(self):
        if self.path.split('?')[0] != '/dev/mock.js':
            return super().send_head()
        script = Path('apps/sunna/dev/mock.js').read_text()
        script = re.sub(r"(?<=address:on\?')[0-9.]+|(?<=link:on\?'sunna://)[0-9.]+",
                        "192.0.2.12", script)
        # Present the app as it runs on Linux: the page's sharing claims are about Linux.
        script = script.replace('platform: "macos"', 'platform: "linux"')
        data = script.encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/javascript')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        return BytesIO(data)

http.server.ThreadingHTTPServer(('127.0.0.1',8795),
    functools.partial(Fixture,directory='apps/sunna')).serve_forever()
