"""The macro smoke tests' fixture server: the folder's files on 127.0.0.1,
plus one server-side redirect, /notes -> /home.html (a site whose notes
page sends you back to the home feed you may already be on).

    python3 serve.py <port> <folder>
"""

import functools
import http.server
import sys


class Handler(http.server.SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path.split("?", 1)[0] == "/notes":
            self.send_response(302)
            self.send_header("Location", "/home.html")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        super().do_GET()

    def log_message(self, *args):
        pass


def main():
    port, folder = int(sys.argv[1]), sys.argv[2]
    handler = functools.partial(Handler, directory=folder)
    http.server.ThreadingHTTPServer(("127.0.0.1", port), handler).serve_forever()


if __name__ == "__main__":
    main()
