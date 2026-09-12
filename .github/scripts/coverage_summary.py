#!/usr/bin/env python3

import argparse
import json
import os
import sys


def percent_value(summary_obj, key):
    metric = summary_obj.get(key, {}) if isinstance(summary_obj, dict) else {}
    value = metric.get("percent", 0.0)
    try:
        return float(value)
    except (TypeError, ValueError):
        return 0.0


def build_markdown(rows):
    lines = []
    lines.append("<!-- coverage-report -->")
    lines.append("### Coverage report summary")
    lines.append("")
    lines.append("| File | Function | Line | Region | Branch |")
    lines.append("| --- | --- | --- | --- | --- |")
    for row in rows:
        lines.append("| " + " | ".join(row) + " |")
    lines.append("")
    lines.append("_Full HTML report is attached to the workflow run as an artifact._")
    return "\n".join(lines)


def parse_coverage_summary(summary_path, max_rows=50):
    with open(summary_path, "r", encoding="utf-8") as fh:
        payload = json.load(fh)

    rows = []
    for entry in payload.get("data", []):
        for file_data in entry.get("files", []):
            filename = file_data.get("filename") or file_data.get("file") or file_data.get("path")
            summary = file_data.get("summary") or {}
            if not filename or not isinstance(summary, dict):
                continue

            rel_name = os.path.relpath(filename, os.getcwd())
            rows.append([
                rel_name,
                f"{percent_value(summary, 'functions'):.1f}%",
                f"{percent_value(summary, 'lines'):.1f}%",
                f"{percent_value(summary, 'regions'):.1f}%",
                f"{percent_value(summary, 'branches'):.1f}%",
            ])

    rows = sorted(rows, key=lambda row: row[0])[:max_rows]
    return rows


def main():
    parser = argparse.ArgumentParser(description="Build a Markdown summary from cargo-llvm-cov JSON coverage data.")
    parser.add_argument("summary_json", help="Path to the llvm-cov summary JSON file")
    parser.add_argument("output_markdown", help="Path to write the generated Markdown summary")
    parser.add_argument("--max-rows", type=int, default=50, help="Maximum number of rows to include in the markdown table")
    args = parser.parse_args()

    if not os.path.exists(args.summary_json):
        print(f"Coverage summary JSON not found: {args.summary_json}", file=sys.stderr)
        return 0

    rows = parse_coverage_summary(args.summary_json, max_rows=args.max_rows)
    if not rows:
        print("No coverage records found in summary JSON")
        return 0

    markdown = build_markdown(rows)
    with open(args.output_markdown, "w", encoding="utf-8") as out:
        out.write(markdown)

    print(f"Wrote Markdown summary to {args.output_markdown}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
