#!/usr/bin/env python3
"""Write the Tauri updater manifest (latest.json) for a GitHub release.

    make-latest-json.py <version> <assets-dir> <release-download-url> [notes-file]

Looks for the signed update bundles in <assets-dir>:
  Linux   *.AppImage + .sig       -> linux-x86_64
  Windows *-setup.exe + .sig      -> windows-x86_64
  macOS   *.app.tar.gz + .sig     -> darwin-x86_64 and darwin-aarch64 (universal)
Exits non-zero if any platform is missing, so a release can never ship
without its update manifest.
"""
import datetime
import json
import pathlib
import sys


def main() -> int:
    version, assets, base = sys.argv[1], pathlib.Path(sys.argv[2]), sys.argv[3].rstrip("/")
    notes = pathlib.Path(sys.argv[4]).read_text() if len(sys.argv) > 4 else ""
    wanted = {
        "linux-x86_64": "*.AppImage",
        "windows-x86_64": "*-setup.exe",
        "darwin-x86_64": "*.app.tar.gz",
        "darwin-aarch64": "*.app.tar.gz",
    }
    platforms, missing = {}, []
    for platform, pattern in wanted.items():
        found = [p for p in sorted(assets.glob(pattern)) if p.with_name(p.name + ".sig").exists()]
        if not found:
            missing.append(f"{platform} ({pattern} with .sig)")
            continue
        bundle = found[0]
        platforms[platform] = {
            "signature": bundle.with_name(bundle.name + ".sig").read_text().strip(),
            "url": f"{base}/{bundle.name}",
        }
    if missing:
        print("missing signed update bundles: " + ", ".join(missing), file=sys.stderr)
        return 1
    manifest = {
        "version": version.lstrip("v"),
        "notes": notes,
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
    }
    (assets / "latest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"latest.json: {', '.join(platforms)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
