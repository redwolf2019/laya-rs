"""Local TLS release fixture for isolated VMs; never configure this CA on a real host.

Generate a temporary self-signed github.com certificate, trust it inside the test VM,
and set https_proxy=http://10.0.2.2:18767 for the installer command only.
The unmodified installer still checks TLS, release versions and all SHA-256 values.
This verifies staged bytes, not public GitHub availability.
"""
import argparse
from http.server import BaseHTTPRequestHandler, SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import ssl


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--cert", required=True)
    parser.add_argument("--key", required=True)
    args = parser.parse_args()
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(args.cert, args.key)

    class Release(SimpleHTTPRequestHandler):
        def __init__(self, *a, **kw):
            super().__init__(*a, directory=str(args.directory), **kw)

        def do_GET(self):
            if self.path.endswith("/releases/latest"):
                self.send_response(302)
                self.send_header("Location", "https://github.com/redwolf2019/laya-rs/releases/tag/v0.1.0")
                self.end_headers()
                return
            if "/releases/tag/" in self.path:
                self.send_response(200)
                self.end_headers()
                return
            prefix = "/bad/" if "/v0.1.1-testfail/" in self.path else "/"
            self.path = prefix + self.path.rsplit("/", 1)[-1]
            super().do_GET()

    class Proxy(BaseHTTPRequestHandler):
        def do_CONNECT(self):
            if self.path != "github.com:443":
                self.send_error(403)
                return
            self.send_response(200, "Connection established")
            self.end_headers()
            with context.wrap_socket(self.connection, server_side=True) as connection:
                Release(connection, self.client_address, self.server)
            self.close_connection = True

    print("Test-only HTTPS proxy on 127.0.0.1:18767", flush=True)
    ThreadingHTTPServer(("127.0.0.1", 18767), Proxy).serve_forever()


if __name__ == "__main__":
    main()
