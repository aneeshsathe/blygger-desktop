#!/usr/bin/env python3
"""Serve Burrow's web build the way a blyg serves it, for development only.

    python3 crates/blyg-web/dev-proxy.py [--port 8791] [--upstream 127.0.0.1:8790] [--mount /notes]

`dist/` is served at {mount}/burrow/ (/burrow/ at the root mount); every
other path ({mount}/api/*, {mount}/studio/*, the blyg's public pages) is
forwarded, as it is, to a LOCAL blyg Worker (`wrangler dev`,
throwaway state) with the browser's own Host, Origin and Sec-Fetch-* headers,
so the owner API's same-origin guard sees exactly what it would see on a real
blyg: the page and the API share one origin, and a cross-origin request is
still refused. Open http://localhost:<port>/burrow/ and sign in at
http://localhost:<port>/studio/login.

A path-mounted blyg (the studio's MOUNT, e.g. "/notes") serves its own
paths under the mount; start its Worker with that MOUNT and pass the same
--mount here.

Never point this at a real blyg. It refuses any upstream that isn't a
loopback address. It logs the method, path and status of each request,
never a header (so never a cookie).

The app's files go out as a blyg on Cloudflare sends them: compressed when
the browser accepts it (Brotli from a `.br` file beside the original, as
serve.sh writes them, else gzip made here), with an ETag, revalidated
(`no-cache`) and answered 304 when unchanged.
"""

import argparse
import gzip
import hashlib
import http.client
import http.server
import ipaddress
import mimetypes
import os
import posixpath
import re
import socket
import sys
import urllib.parse

HERE = os.path.dirname(os.path.abspath(__file__))
# What a blyg on Cloudflare compresses (text, JSON, wasm, SVG), here.
COMPRESSIBLE = ("text/", "application/javascript", "application/json", "application/wasm",
                "application/manifest+json", "image/svg+xml")
HOP = {"connection", "keep-alive", "proxy-authenticate", "proxy-authorization", "te",
       "trailers", "transfer-encoding", "upgrade", "content-length"}


def loopback(host):
    try:
        return all(ipaddress.ip_address(a[4][0]).is_loopback
                   for a in socket.getaddrinfo(host, None))
    except (OSError, ValueError):
        return False


def accepts(header, coding):
    """Whether an Accept-Encoding header allows `coding` (q=0 refuses it)."""
    for part in (header or "").split(","):
        name, _, params = part.strip().partition(";")
        if name.strip().lower() == coding:
            return params.replace(" ", "").lower() not in ("q=0", "q=0.0", "q=0.00", "q=0.000")
    return False


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    gz = {}  # path -> (mtime, gzipped bytes)
    dist = os.path.join(HERE, "dist")
    upstream = ("127.0.0.1", 8790)
    mount = ""

    def log_message(self, fmt, *args):  # method, path, status only
        sys.stderr.write("%s\n" % (fmt % args))

    def _any(self):
        path = urllib.parse.urlsplit(self.path).path
        app = self.mount + "/burrow/"
        if path in ("/", self.mount, self.mount + "/", app[:-1]):
            self.send_response(302)
            self.send_header("Location", app)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        if path.startswith(app):
            self._static(path[len(app):])
        else:
            self._forward()

    do_GET = do_HEAD = do_POST = do_PUT = do_PATCH = do_DELETE = do_OPTIONS = _any

    def _static(self, rel):
        if self.command not in ("GET", "HEAD"):
            return self._plain(405, "method not allowed")
        rel = posixpath.normpath(urllib.parse.unquote(rel or "index.html")).lstrip("/")
        if rel in ("", "."):
            rel = "index.html"
        full = os.path.realpath(os.path.join(self.dist, rel))
        if not full.startswith(os.path.realpath(self.dist) + os.sep) or not os.path.isfile(full):
            return self._plain(404, "not found")
        with open(full, "rb") as f:
            data = f.read()
        ctype = mimetypes.guess_type(full)[0] or "application/octet-stream"
        if full.endswith(".wasm"):
            ctype = "application/wasm"
        elif full.endswith(".webmanifest"):
            ctype = "application/manifest+json"
        elif full.endswith(".js"):
            ctype = "text/javascript"
        etag = '"%s"' % hashlib.sha256(data).hexdigest()[:20]
        coding = None
        if ctype.startswith(COMPRESSIBLE) and len(data) > 1024:
            ae = self.headers.get("Accept-Encoding")
            if accepts(ae, "br") and os.path.isfile(full + ".br"):
                coding = "br"
                with open(full + ".br", "rb") as f:
                    data = f.read()
            elif accepts(ae, "gzip"):
                coding = "gzip"
                mtime = os.path.getmtime(full)
                hit = self.gz.get(full)
                if not hit or hit[0] != mtime:
                    hit = (mtime, gzip.compress(data, 6, mtime=0))
                    self.gz[full] = hit
                data = hit[1]
        if coding:
            etag = etag[:-1] + '-' + coding + '"'
        if etag in [t.strip() for t in (self.headers.get("If-None-Match") or "").split(",")]:
            self.send_response(304)
            self.send_header("ETag", etag)
            self.send_header("Cache-Control", "no-cache")
            self.send_header("Vary", "Accept-Encoding")
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-cache")
        self.send_header("ETag", etag)
        self.send_header("Vary", "Accept-Encoding")
        if coding:
            self.send_header("Content-Encoding", coding)
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(data)

    def _forward(self):
        n = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(n) if n else None
        headers = {k: v for k, v in self.headers.items() if k.lower() not in HOP}
        try:
            conn = http.client.HTTPConnection(*self.upstream, timeout=60)
            conn.request(self.command, self.path, body=body, headers=headers)
            res = conn.getresponse()
            data = res.read()
        except OSError:
            return self._plain(502, "the local blyg isn't answering")
        self.send_response(res.status, res.reason)
        for k, v in res.getheaders():
            if k.lower() not in HOP:
                self.send_header(k, v)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(data)
        conn.close()

    def _plain(self, status, text):
        data = text.encode()
        self.send_response(status)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        if self.command != "HEAD":
            self.wfile.write(data)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--port", type=int, default=8791)
    ap.add_argument("--upstream", default="127.0.0.1:8790")
    ap.add_argument("--dist", default=os.path.join(HERE, "dist"))
    ap.add_argument("--mount", default="", help='the blyg\'s path mount, e.g. "/notes" (default: the root)')
    a = ap.parse_args()
    if a.mount and not re.fullmatch(r"(/[A-Za-z0-9._~-]+)+", a.mount.rstrip("/")):
        sys.exit("dev-proxy: --mount is a path like /notes")
    Handler.mount = a.mount.rstrip("/")
    host, _, port = a.upstream.rpartition(":")
    if not loopback(host):
        sys.exit("dev-proxy: the upstream must be a local blyg (a loopback address), never a real one")
    Handler.upstream = (host, int(port))
    Handler.dist = a.dist
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", a.port), Handler)
    print(f"Burrow at http://localhost:{a.port}{Handler.mount}/burrow/ · local blyg at {a.upstream}", flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
