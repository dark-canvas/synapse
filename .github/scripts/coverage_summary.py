#!/usr/bin/env python3

import argparse
import json
import os
import sys

# map of percent coverage to the colour it should appear as
COVERAGE_CATEGORIES = {
   80: "lime",
   60: "yellow",
    0: "orangered",
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


def build_markdown(rows, head_totals=None, baseline_totals=None):
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


def compute_totals_from_payload(payload):
    totals = {
        'functions': {'covered': 0, 'count': 0},
        'lines': {'covered': 0, 'count': 0},
        'regions': {'covered': 0, 'count': 0},
        'branches': {'covered': 0, 'count': 0},
    }
    for entry in payload.get('data', []):
        for file_data in entry.get('files', []):
            summary = file_data.get('summary') or {}
            for key in totals.keys():
                metric = summary.get(key) or {}
                covered = metric.get('covered')
                count = metric.get('count')
                # Some summaries may only have percent; skip those files for totals
                if covered is None or count is None:
                    continue
                totals[key]['covered'] += covered
                totals[key]['count'] += count
    return totals


def percent_from_totals(tot):
    if tot['count'] == 0:
        return 0.0
    return (tot['covered'] / tot['count']) * 100.0


def parse_coverage_summary(summary_path, max_rows=50, changed_files=None, baseline_path=None):
    with open(summary_path, "r", encoding="utf-8") as fh:
        payload = json.load(fh)

    baseline_map = {}
    baseline_totals = None
    if baseline_path and os.path.exists(baseline_path):
        try:
            with open(baseline_path, 'r', encoding='utf-8') as bf:
                base_payload = json.load(bf)
            # build per-file percent map from baseline
            for entry in base_payload.get('data', []):
                for file_data in entry.get('files', []):
                    filename = file_data.get('filename') or file_data.get('file') or file_data.get('path')
                    summary = file_data.get('summary') or {}
                    if not filename or not isinstance(summary, dict):
                        continue
                    rel_name = os.path.relpath(filename, os.getcwd())
                    baseline_map[os.path.normpath(rel_name)] = {
                        'functions': float(summary.get('functions', {}).get('percent', 0.0) or 0.0),
                        'lines': float(summary.get('lines', {}).get('percent', 0.0) or 0.0),
                        'regions': float(summary.get('regions', {}).get('percent', 0.0) or 0.0),
                        'branches': float(summary.get('branches', {}).get('percent', 0.0) or 0.0),
                    }
            baseline_totals = compute_totals_from_payload(base_payload)
        except Exception:
            baseline_map = {}
            baseline_totals = None

    changed_set = set()
    if changed_files and os.path.exists(changed_files):
        with open(changed_files, 'r', encoding='utf-8') as cf:
            for line in cf:
                line = line.strip()
                if not line:
                    continue
                changed_set.add(os.path.normpath(line))

    grouped_rows = []
    for entry in payload.get("data", []):
        for file_data in entry.get("files", []):
            filename = file_data.get("filename") or file_data.get("file") or file_data.get("path")
            summary = file_data.get("summary") or {}
            if not filename or not isinstance(summary, dict):
                continue

            rel_name = os.path.relpath(filename, os.getcwd())
            norm_rel = os.path.normpath(rel_name)
            functions_pct = percent_value(summary, 'functions')
            lines_pct = percent_value(summary, 'lines')
            regions_pct = percent_value(summary, 'regions')
            branches_pct = percent_value(summary, 'branches')

            display_name = rel_name
            delta_row = None
            if norm_rel in changed_set:
                base = baseline_map.get(norm_rel)
                if base:
                    df = functions_pct - base.get('functions', 0.0)
                    dl = lines_pct - base.get('lines', 0.0)
                    dr = regions_pct - base.get('regions', 0.0)
                    db = branches_pct - base.get('branches', 0.0)
                    display_name = f"**🔷 {rel_name}**"
                    delta_row = [
                        "Δ vs main",
                        f"{df:+.1f}pp",
                        f"{dl:+.1f}pp",
                        f"{dr:+.1f}pp",
                        f"{db:+.1f}pp",
                    ]
                else:
                    display_name = f"**🔷 {rel_name}**"

            row = [
                display_name,
                f"{functions_pct:.1f}%",
                f"{lines_pct:.1f}%",
                f"{regions_pct:.1f}%",
                f"{branches_pct:.1f}%",
            ]
            grouped_rows.append((norm_rel, row))
            if delta_row is not None:
                grouped_rows.append((norm_rel, delta_row))

    grouped_rows = sorted(grouped_rows, key=lambda item: item[0])
    rows = []
    for _, row in grouped_rows[:max_rows * 2]:
        rows.append(row)
    head_totals = compute_totals_from_payload(payload)
    return rows, head_totals, baseline_totals

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
    parser.add_argument("--baseline", type=str, default="", help="Optional baseline coverage JSON to compare against")
    args = parser.parse_args()

    if not os.path.exists(args.summary_json):
        print(f"Coverage summary JSON not found: {args.summary_json}", file=sys.stderr)
        return 0

    rows, head_totals, baseline_totals = parse_coverage_summary(
        args.summary_json,
        max_rows=args.max_rows,
        changed_files=args.changed_files,
        baseline_path=args.baseline if hasattr(args, 'baseline') else None,
    )
    if not rows:
        print("No coverage records found in summary JSON")
        return 0

    markdown = build_markdown(rows, head_totals=head_totals, baseline_totals=baseline_totals)
    with open(args.output_markdown, "w", encoding="utf-8") as out:
        out.write(markdown)

    print(f"Wrote Markdown summary to {args.output_markdown}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
