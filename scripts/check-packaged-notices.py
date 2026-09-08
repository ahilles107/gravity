#!/usr/bin/env python3
"""Verify actual app resources, embedded sidecar notices and daemon tar contents."""

import argparse
import pathlib
import subprocess
import tarfile

ROOT = pathlib.Path(__file__).resolve().parents[1]


def require_equal(actual, expected, label):
    if actual != expected:
        raise ValueError("Packaged notice mismatch: " + label)


def check(app, daemon):
    desktop_notice = (ROOT / "third-party/DESKTOP_NOTICES.txt").read_bytes()
    daemon_notice = (ROOT / "third-party/DAEMON_NOTICES.txt").read_bytes()
    license_text = (ROOT / "LICENSE").read_bytes()
    resources = app / "Contents/Resources"
    require_equal((resources / "THIRD_PARTY_NOTICES.txt").read_bytes(), desktop_notice, "app")
    require_equal((resources / "LICENSE").read_bytes(), license_text, "app MIT license")
    embedded = subprocess.check_output([app / "Contents/MacOS/gravityd", "--third-party-notices"])
    require_equal(embedded, daemon_notice, "sidecar")
    with tarfile.open(daemon) as archive:
        for filename, expected in [("THIRD_PARTY_NOTICES.txt", daemon_notice), ("LICENSE", license_text)]:
            entries = [m for m in archive.getmembers() if pathlib.PurePosixPath(m.name).name == filename]
            if len(entries) != 1 or not entries[0].isfile():
                raise ValueError("Missing or ambiguous daemon archive notice: " + filename)
            stream = archive.extractfile(entries[0])
            if stream is None:
                raise ValueError("Unreadable daemon archive notice: " + filename)
            require_equal(stream.read(), expected, filename)
    print("App resources, embedded sidecar and daemon archive notices verified.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", required=True, type=pathlib.Path)
    parser.add_argument("--daemon", required=True, type=pathlib.Path)
    args = parser.parse_args()
    check(args.app, args.daemon)
