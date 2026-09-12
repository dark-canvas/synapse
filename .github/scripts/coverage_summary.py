#!/usr/bin/env python3

import argparse
import json
import os
import sys

# map of percent coverage to the colour it should appear as
COVERAGE_CATEGORIES = {
    80: "DarkGreen",
    60: "DarkOrange",
     0: "DarkRed",
}


def coverage_color_for(value):
   """Return the background colour for a coverage percentage."""
   try:
       numeric_value = float(value)
   except (TypeError, ValueError):
       numeric_value = 0.0

   if numeric_value >= 80:
       return COVERAGE_CATEGORIES[80]
   if numeric_value >= 60:
       return COVERAGE_CATEGORIES[60]
   return COVERAGE_CATEGORIES[0]


def styled_cell(value):
   """Render a percentage cell with a background colour and white foreground text."""
   if not isinstance(value, str):
       return str(value)

   try:
       numeric_value = float(value.rstrip("%"))
   except ValueError:
       return value

   colour = coverage_color_for(numeric_value)
   return (
       f'<span style="display: inline-block; background-color: {colour}; '
       'color: white; padding: 0.15em 0.45em; border-radius: 0.25rem; '
       'font-weight: 600;">'
       f'{value}</span>'
   )


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
        cells = [row[0]] + [styled_cell(cell) for cell in row[1:]]
        lines.append("| " + " | ".join(cells) + " |")
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


# TODO: compare against previous baseline and show difference?
# TODO: display summary only for files which have changed in the PR?
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
