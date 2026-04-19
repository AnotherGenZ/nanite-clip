#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--key-id", required=False)
    parser.add_argument("--keys-json-env", required=False)
    parser.add_argument("--fallback-pem-env", required=False)
    parser.add_argument("--output", required=True)
    return parser.parse_args()


def load_key_from_json(env_name: str | None, key_id: str | None) -> str | None:
    if not env_name:
        return None
    raw = os.environ.get(env_name, "").strip()
    if not raw:
        return None

    try:
        parsed = json.loads(raw)
    except json.JSONDecodeError as error:
        raise SystemExit(f"{env_name} did not contain valid JSON: {error}") from error

    if not isinstance(parsed, dict):
        raise SystemExit(f"{env_name} must be a JSON object that maps key ids to PEM strings")
    if not key_id:
        raise SystemExit(
            f"{env_name} was set, but --key-id was not provided to choose a signing key"
        )

    pem = parsed.get(key_id)
    if pem is None:
        available = ", ".join(sorted(parsed.keys()))
        raise SystemExit(
            f"{env_name} did not contain a signing key for '{key_id}'. Available ids: {available}"
        )
    if not isinstance(pem, str) or not pem.strip():
        raise SystemExit(f"{env_name}[{key_id!r}] must be a non-empty PEM string")
    return pem


def main():
    args = parse_args()

    pem = load_key_from_json(args.keys_json_env, args.key_id)
    if pem is None and args.fallback_pem_env:
        pem = os.environ.get(args.fallback_pem_env, "").strip() or None

    if pem is None:
        raise SystemExit(
            "No update signing key was available. Set a JSON key ring secret or a fallback PEM secret."
        )

    Path(args.output).write_text(pem + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
