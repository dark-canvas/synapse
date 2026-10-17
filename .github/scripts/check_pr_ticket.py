#!/usr/bin/env python3

import json
import os
import re
import subprocess
import sys

# for now, ignore small changes.  In the future I may require a specific opt-out instead, 
# such as "#no-ticket"
THRESHOLD_LINES = 100
ISSUE_REF_RE = re.compile(
    r"(?<![A-Za-z0-9_])(?:https://github\.com/)?(?:"
    r"(?P<owner>[A-Za-z0-9_.-]+)/(?P<repo>[A-Za-z0-9_.-]+))?#(?P<number>\d+)"
)


def run(*args: str) -> str:
    result = subprocess.run(args, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(
            f"Command failed ({result.returncode}): {' '.join(args)}\n"
            f"stdout: {result.stdout}\n"
            f"stderr: {result.stderr}"
        )
    return result.stdout.strip()


def extract_issues_from_message(message: str, owner: str, repo: str):
    refs = set()
    for match in ISSUE_REF_RE.finditer(message):
        ref_owner = match.group("owner") or owner
        ref_repo = match.group("repo") or repo
        number = match.group("number")
        if ref_owner.lower() != owner.lower() or ref_repo.lower() != repo.lower():
            continue
        refs.add(int(number))
    return sorted(refs)


def iter_large_commits(base_sha: str, head_sha: str):
    raw = run(
        "git",
        "log",
        "--reverse",
        "--no-merges",
        f"{base_sha}..{head_sha}",
        "--pretty=format:%H%x1f%B%x1e",
    )

    for entry in raw.split("\x1e"):
        if not entry.strip():
            continue
        parts = entry.split("\x1f", 1)
        if len(parts) != 2:
            continue
        sha, subject = parts
        sha = sha.strip()
        message = subject.strip()
        if not sha or not message:
            continue

        numstat = run("git", "show", "--numstat", "--format=", "--no-renames", sha)
        total = 0
        for line in numstat.splitlines():
            pieces = line.split("\t")
            if len(pieces) < 3:
                continue
            try:
                added = int(pieces[0])
                removed = int(pieces[1])
            except ValueError:
                continue
            total += added + removed

        if total > THRESHOLD_LINES:
            yield sha, message, total


def validate_issue(repo: str, issue_number: int):
    payload = run(
        "gh",
        "api",
        f"repos/{repo}/issues/{issue_number}",
        "--jq",
        "{number: .number, pull_request: (.pull_request != null), state: .state, title: .title}",
    )
    if not payload:
        return False

    try:
        data = json.loads(payload)
    except json.JSONDecodeError:
        return False

    return data.get("number") == issue_number and data.get("pull_request") is False


def main():
    repo = os.environ.get("GH_REPO")
    base_sha = os.environ.get("PR_BASE_SHA")
    head_sha = os.environ.get("PR_HEAD_SHA")
    if not repo or not base_sha or not head_sha:
        print("GH_REPO, PR_BASE_SHA, and PR_HEAD_SHA are required.", file=sys.stderr)
        return 2

    large_commits = list(iter_large_commits(base_sha, head_sha))
    if not large_commits:
        print(f"No commit exceeds {THRESHOLD_LINES} changed lines; no ticket required.")
        return 0

    failures = []
    for sha, message, total in large_commits:
        refs = extract_issues_from_message(message, repo.split("/", 1)[0], repo.split("/", 1)[1])
        valid_refs = [n for n in refs if validate_issue(repo, n)]
        if not valid_refs:
            failures.append((sha, total, message, refs))

    if failures:
        print(
            f"Large commits (> {THRESHOLD_LINES} changed lines) must reference a valid issue in the repository:",
            file=sys.stderr,
        )
        for sha, total, message, refs in failures:
            print(f"- {sha[:12]} ({total} changed lines): {message}", file=sys.stderr)
            if refs:
                print(f"  References found: {', '.join(f'#{n}' for n in refs)}", file=sys.stderr)
            print("  Required: a GitHub issue in this repository, e.g. #123 or owner/repo#123", file=sys.stderr)
        return 1

    print(f"Validated {len(large_commits)} large commit(s) with valid issue references.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as exc:
        print(str(exc), file=sys.stderr)
        raise SystemExit(1)
