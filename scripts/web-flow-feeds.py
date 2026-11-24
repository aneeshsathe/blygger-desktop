#!/usr/bin/env python3
"""Plain RSS sites for the web flow pass (scripts/browser-web-flow-pass.sh).

A reading list of realistic volume needs many subscriptions, and each
subscription is an origin. Starting a studio per origin is heavy, so the
pass follows one real blyg (`b`) and these: COUNT small sites on loopback
ports, each a home page with RSS autodiscovery and an RSS 2.0 feed (the
studio's "L0" legacy-feed path, the same as any blog without Blygger).

Invented content only: long paragraphs, images with and without a size,
quotes inside quotes, code, lists. Nothing here is a studio or a mock of
one; the studio under test reads these as it reads any website.

    web-flow-feeds.py --count 11 --entries 30 --out <sites.json>

Writes each site's port and name to --out once every site answers, then
serves until killed. Loopback only.
"""

import argparse
import json
import struct
import sys
import threading
import zlib
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from xml.sax.saxutils import escape

TOPICS = [
    "tide pools", "kelp forests", "sea otters", "barnacles", "sand dunes", "lichen",
    "salt marshes", "estuaries", "mudflats", "driftwood", "sea glass", "rock pools",
]
NAMES = [
    "Coastal notebook", "Field journal", "Low tide letters", "The quiet shore", "Marsh weekly",
    "Dune diary", "Kelp and stone", "The otter log", "Estuary notes", "Pebble press",
    "Salt and wind", "Shoreline sketches",
]


def png(w, h, rgb):
    """A solid-colour PNG, so posts carry real images without files."""
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))

    def chunk(kind, data):
        c = kind + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def entry_html(site, i, base):
    t = TOPICS[(site + i) % len(TOPICS)]
    para = (f"An invented paragraph about {t}, written for a test of how reading feels. " * 9).strip()
    parts = [f"<p>{para}</p>"]
    if i % 3 == 0:
        # Half the images say their size and half don't, as on the web.
        size = ' width="640" height="320"' if i % 2 == 0 else ""
        parts.append(f'<p><img src="{base}/img/{i % 4}.png" alt="An invented picture of {t}"{size}></p>')
    if i % 4 == 1:
        parts.append(
            "<blockquote><p>Someone else said something invented about "
            f"{t}.</p><blockquote><p>And before them, someone said it first, "
            "in a quote inside the quote.</p><blockquote><p>A third level, "
            "because threads go deep.</p></blockquote></blockquote></blockquote>")
    if i % 5 == 2:
        parts.append("<pre><code>const shore = [" + ", ".join(f'"{t} {n}"' for n in range(8)) + "];</code></pre>")
    if i % 2 == 0:
        parts.append("<ul>" + "".join(f"<li>An invented point {n} on {t}.</li>" for n in range(1, 5)) + "</ul>")
    parts.append(f"<p>{(f'A closing thought on {t}, also invented. ' * 6).strip()}</p>")
    return "".join(parts)


class Site:
    def __init__(self, n, entries, start):
        self.n = n
        self.name = NAMES[n % len(NAMES)]
        self.entries = entries
        self.start = start
        self.port = 0

    def feed(self):
        base = f"http://127.0.0.1:{self.port}"
        items = []
        for i in range(self.entries):
            when = self.start - timedelta(hours=(self.entries - i) * 5 + self.n)
            t = TOPICS[(self.n + i) % len(TOPICS)]
            items.append(
                "<item>"
                f"<title>{escape(f'{self.name} {i + 1}: on {t}')}</title>"
                f"<link>{base}/posts/{i + 1}</link>"
                f"<guid>{base}/posts/{i + 1}</guid>"
                f"<pubDate>{when.strftime('%a, %d %b %Y %H:%M:%S +0000')}</pubDate>"
                f"<description>{escape(entry_html(self.n, i, base))}</description>"
                "</item>")
        return ('<?xml version="1.0" encoding="utf-8"?><rss version="2.0"><channel>'
                f"<title>{escape(self.name)}</title><link>{base}/</link>"
                f"<description>Invented sample feed {self.n + 1}</description>"
                + "".join(reversed(items)) + "</channel></rss>").encode()

    def home(self):
        return (f'<!doctype html><html><head><meta charset="utf-8"><title>{escape(self.name)}</title>'
                '<link rel="alternate" type="application/rss+xml" href="/feed.xml"></head>'
                f"<body><h1>{escape(self.name)}</h1><p>An invented site.</p></body></html>").encode()


IMAGES = [png(64, 32, c) for c in [(200, 170, 120), (90, 140, 170), (120, 170, 110), (170, 120, 150)]]


def handler_for(site):
    class H(BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def do_GET(self):
            p = self.path.split("?")[0]
            if p in ("/", "/index.html") or p.startswith("/posts/"):
                body, ctype = site.home(), "text/html; charset=utf-8"
            elif p in ("/feed.xml", "/rss.xml"):
                body, ctype = site.feed(), "application/rss+xml; charset=utf-8"
            elif p.startswith("/img/") and p.endswith(".png"):
                try:
                    body, ctype = IMAGES[int(p[5:-4]) % len(IMAGES)], "image/png"
                except ValueError:
                    self.send_error(404)
                    return
            else:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("content-type", ctype)
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        do_HEAD = do_GET

    return H


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--count", type=int, default=10)
    ap.add_argument("--entries", type=int, default=30)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    start = datetime.now(timezone.utc)
    servers = []
    for n in range(a.count):
        site = Site(n, a.entries, start)
        srv = ThreadingHTTPServer(("127.0.0.1", 0), handler_for(site))
        site.port = srv.server_address[1]
        servers.append(srv)
        threading.Thread(target=srv.serve_forever, daemon=True).start()
    with open(a.out, "w") as f:
        json.dump([{"port": srv.server_address[1], "name": NAMES[n % len(NAMES)]} for n, srv in enumerate(servers)], f)
    sys.stdout.write(f"web-flow-feeds: {a.count} sites up\n")
    sys.stdout.flush()
    threading.Event().wait()


if __name__ == "__main__":
    main()
