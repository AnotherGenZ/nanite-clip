#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--checksums", required=True)
    parser.add_argument("--output", required=True)
    return parser.parse_args()


def load_checksums(path: Path):
    mapping = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        digest, filename = line.split("  ", 1)
        mapping[Path(filename).name] = digest
    return mapping


def main():
    args = parse_args()
    tag = args.tag
    version = tag[1:] if tag.startswith("v") else tag
    checksums = load_checksums(Path(args.checksums))

    assets = []
    candidates = [
        ("windows_msi", "msi", f"nanite-clip-{tag}-x86_64.msi"),
        ("windows_portable", "exe", "nanite-clip.exe"),
        ("linux_portable", "tar_gz", f"nanite-clip-{tag}-x86_64-linux.tar.gz"),
    ]
    for channel, kind, filename in candidates:
        digest = checksums.get(filename)
        if not digest:
            continue
        assets.append(
            {
                "channel": channel,
                "kind": kind,
                "filename": filename,
                "download_url": f"https://github.com/{args.repo}/releases/download/{tag}/{filename}",
                "sha256": digest,
            }
        )

    manifest = {
        "version": version,
        "tag_name": tag,
        "release_notes_url": f"https://github.com/{args.repo}/releases/tag/{tag}",
        "assets": assets,
    }

    Path(args.output).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
