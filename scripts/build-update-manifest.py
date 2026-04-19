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
    parser.add_argument("--signing-key-id")
    parser.add_argument("--signing-key-label")
    parser.add_argument("--minimum-supported-version")
    parser.add_argument(
        "--blocked-version",
        action="append",
        default=[],
        help="Repeat to add blocked current versions for this release.",
    )
    parser.add_argument("--rollout-percentage", type=int)
    parser.add_argument(
        "--mandatory",
        action="store_true",
        help="Mark this release as mandatory in the signed update manifest.",
    )
    parser.add_argument("--message")
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
    if args.rollout_percentage is not None and not 0 <= args.rollout_percentage <= 100:
        raise SystemExit("--rollout-percentage must be between 0 and 100")

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
        "minimum_supported_version": args.minimum_supported_version,
        "blocked_versions": args.blocked_version,
        "mandatory": args.mandatory,
        "message": args.message,
        "signature": {
            "algorithm": "ed25519",
            "key_id": args.signing_key_id,
            "key_label": args.signing_key_label,
        },
        "assets": assets,
    }
    if args.rollout_percentage is not None:
        manifest["rollout"] = {"percentage": args.rollout_percentage}

    Path(args.output).write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
