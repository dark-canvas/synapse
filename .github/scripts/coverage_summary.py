#!/usr/bin/env python3

import argparse
import json
import os
import sys

# map of percent coverage to the colour it should appear as
COVERAGE_CATEGORIES = {
   80: "lime",
   60: "orangered",
    0: "crimson",
}


def coverage_color_for(value):
    """Return the text colour for a coverage percentage."""
    try:
        numeric_value = float(value)
    except (TypeError, ValueError):
        numeric_value = 0.0

    text_color = "white"
    delta = 100
    for percent, colour in COVERAGE_CATEGORIES.items():
        compare_delta = numeric_value - percent
        if compare_delta >= 0 and compare_delta < delta:
            delta = compare_delta
            text_color = colour

    return text_color


def styled_cell(value):
    """Render a percentage cell using GitHub KaTeX text colouring."""
    if not isinstance(value, str):
        return str(value)

    try:
        numeric_value = float(value.rstrip("%"))
    except ValueError:
        return value

    value = value.replace("%", r"\\%")

    colour = coverage_color_for(numeric_value)
    return f"$$\\color{{{colour}}}\\text{{{value}}}$$"


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
        # first column already contains the display name (may include change marker)
        file = row[0]
        cells = [file] + [styled_cell(cell) for cell in row[1:]]
        lines.append("| " + " | ".join(cells) + " |")
    lines.append("")
    lines.append("_Full HTML report is attached to the workflow run as an artifact._")
    return "\n".join(lines)


def parse_coverage_summary(summary_path, max_rows=50, changed_files=None):
    with open(summary_path, "r", encoding="utf-8") as fh:
        payload = json.load(fh)

    changed_set = set()
    if changed_files and os.path.exists(changed_files):
        with open(changed_files, 'r', encoding='utf-8') as cf:
            for line in cf:
                line = line.strip()
                if not line:
                    continue
                changed_set.add(os.path.normpath(line))

    rows = []
    for entry in payload.get("data", []):
        for file_data in entry.get("files", []):
            filename = file_data.get("filename") or file_data.get("file") or file_data.get("path")
            summary = file_data.get("summary") or {}
            if not filename or not isinstance(summary, dict):
                continue

            rel_name = os.path.relpath(filename, os.getcwd())
            norm_rel = os.path.normpath(rel_name)
            display_name = rel_name
            if norm_rel in changed_set:
                display_name = f"**🔷 {rel_name}**"

            rows.append([
                display_name,
                f"{percent_value(summary, 'functions'):.1f}%",
                f"{percent_value(summary, 'lines'):.1f}%",
                f"{percent_value(summary, 'regions'):.1f}%",
                f"{percent_value(summary, 'branches'):.1f}%",
            ])

    rows = sorted(rows, key=lambda row: row[0])[:max_rows]
    return rows

def read_file_list(fname):
    files = []
    if fname != "":
        with open(fname, 'r') as f:
            files = f.read().splitlines()

    print(f"files: {files}")

    return files

# TODO: compare against previous baseline and show difference?
# TODO: display summary only for files which have changed in the PR?
def main():
    parser = argparse.ArgumentParser(description="Build a Markdown summary from cargo-llvm-cov JSON coverage data.")
    parser.add_argument("summary_json", help="Path to the llvm-cov summary JSON file")
    parser.add_argument("output_markdown", help="Path to write the generated Markdown summary")
    parser.add_argument("--max-rows", type=int, default=50, help="Maximum number of rows to include in the markdown table")
    parser.add_argument("--changed-files", type=str, default="", help="List of files changed in this PR")
    args = parser.parse_args()

    if not os.path.exists(args.summary_json):
        print(f"Coverage summary JSON not found: {args.summary_json}", file=sys.stderr)
        return 0

    rows = parse_coverage_summary(args.summary_json, max_rows=args.max_rows, changed_files=args.changed_files)
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
