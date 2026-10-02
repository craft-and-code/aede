#!/usr/bin/env python3
"""Serve the generated static website without introducing dependencies."""
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=4174)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--directory", type=Path, default=Path(__file__).resolve().parent.parent / "dist-site")
    args = parser.parse_args()
    if not (args.directory / "index.html").is_file(): parser.error("Build the site first: python3 tools/build-site.py --check")
    with ThreadingHTTPServer((args.host, args.port), partial(SimpleHTTPRequestHandler, directory=str(args.directory))) as server:
        print(f"Aède website preview: http://{args.host}:{server.server_port}/")
        try: server.serve_forever()
        except KeyboardInterrupt: pass


if __name__ == "__main__": main()
