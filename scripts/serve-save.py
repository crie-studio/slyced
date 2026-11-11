"""Local helper for pages that generate files (scripts/make-og.html): serves the
repository and saves what a page POSTs to /salvar/<name> into the given folder.
Listens on 127.0.0.1 only.

    python scripts/serve-save.py 8767 web/landing
"""
import http.server
import os
import re
import sys

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
OUT = os.path.abspath(os.path.join(REPO, sys.argv[2] if len(sys.argv) > 2 else "."))


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=REPO, **kwargs)

    def do_POST(self):
        m = re.fullmatch(r"/salvar/([A-Za-z0-9_.-]+)", self.path)
        if not m:
            self.send_error(404)
            return
        size = int(self.headers.get("Content-Length", "0"))
        os.makedirs(OUT, exist_ok=True)
        with open(os.path.join(OUT, m.group(1)), "wb") as f:
            f.write(self.rfile.read(size))
        self.send_response(204)
        self.end_headers()


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8767
    http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
