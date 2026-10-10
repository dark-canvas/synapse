#!/usr/bin/env python3

import re
import sys
from pathlib import Path

ALLOWED_MODULES = {
    "build",
    "core",
    "docs",
    "init",
    "pager",
    "page-based",
    "scheduler",
    "smp",
    "sync",
}

ALLOWED_PLATFORMS = {"x86-64", "amd64"}
ALLOWED_PLATFORM_MODULES = {
    "x86-64" : {
        "gdt",
        "idt",
    }
}

SUBJECT_REGEX = re.compile(r"^[a-z0-9-]+(?:/[a-z0-9-]+)*: [^\n]+$")
MAX_SUBJECT_LENGTH = 120

def iter_commit_messages(path: Path):
    raw = path.read_text(encoding="utf-8")
    entries = raw.split("\x1e")
    for entry in entries:
        message = entry.strip()
        if message and not message.startswith("Merge "):
            yield message


def validate_message(message: str):
    errors = []
    lines = message.splitlines()

    if not lines:
        return ["Commit message is empty."]

    subject = lines[0].strip()
    if not subject:
        return ["Commit subject is empty."]

    if len(subject) > MAX_SUBJECT_LENGTH:
        errors.append(f"Subject exceeds {MAX_SUBJECT_LENGTH} characters ({len(subject)}): {subject}")

    if not SUBJECT_REGEX.match(subject):
        errors.append(
            "Subject must match 'module: summary' or 'platform/module: summary'; "
            f"got: {subject}"
        )
        return errors

    # the regex ensures this will always succeed
    name, _, _summary = subject.partition(": ")

    if "/" in name:
        platform, module = name.split("/", 1)
        if platform not in ALLOWED_PLATFORMS or \
            (
                module not in ALLOWED_PLATFORM_MODULES[platform] and 
                module not in ALLOWED_MODULES
            ):
            errors.append(
                "Platform-specific subjects must use a supported platform and allowed module; "
                f"got: {subject}. Allowed platforms: {sorted(ALLOWED_PLATFORMS)}. "
                f"Allowed modules: {sorted(ALLOWED_MODULES | ALLOWED_PLATFORM_MODULES[platform])}"
            )
    elif name not in ALLOWED_MODULES and name not in ALLOWED_PLATFORMS:
        errors.append(
            "Module must be one of the documented project modules; "
            f"got: {subject}. Allowed modules: {sorted(ALLOWED_MODULES | ALLOWED_PLATFORMS)}"
        )

    return errors


def main():
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <commit-messages-file>", file=sys.stderr)
        return 2

    path = Path(sys.argv[1])
    if not path.exists():
        print(f"Commit message file not found: {path}", file=sys.stderr)
        return 2

    failures = []
    total = 0

    for message in iter_commit_messages(path):
        total += 1
        errors = validate_message(message)
        if errors:
            failures.append((message, errors))

    if total == 0:
        print("No commit messages to validate.")
        return 0

    if failures:
        print(f"Commit message validation failed for {len(failures)} commit(s):")
        for message, errors in failures:
            print(f"\n---")
            print(message)
            for error in errors:
                print(f"  - {error}")
        return 1

    print(f"Validated {total} commit message(s) successfully.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
