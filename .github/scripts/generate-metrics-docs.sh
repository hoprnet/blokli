#!/usr/bin/env bash
# Lint: ensures every Prometheus metric defined in Rust code is documented in
# METRICS.md, and that METRICS.md does not reference non-existing metrics.
#
# Usage:  ./.github/scripts/generate-metrics-docs.sh              (lint — verify sync)
#         ./.github/scripts/generate-metrics-docs.sh --fix         (update METRICS.md and git-add)
#         ./.github/scripts/generate-metrics-docs.sh --generate    (print markdown table to stdout)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
METRICS_DOC="$REPO_ROOT/METRICS.md"

# ── Helper: extract metrics from code ────────────────────────────────────────
# Outputs tab-separated rows:  name \t type \t description \t detail
# Requires 8 lines of trailing context to capture buckets/keys on later lines.
extract_metrics() {
  (cd "$REPO_ROOT" && grep -hrA8 -E '(Simple|Multi)(Counter|Gauge|Histogram)::new\(' --exclude-dir='.git' --exclude-dir='.claude' --exclude-dir='target' --exclude-dir='.cargo' --include='*.rs' .) |
    gawk '
    function flush_metric(    detail, escaped_desc, escaped_detail) {
      if (name != "") {
        if (collecting_desc && desc == "" && desc_buffer != "") {
          desc = desc_buffer
          gsub(/[[:space:]]+/, " ", desc)
          sub(/^ /, "", desc)
          sub(/ $/, "", desc)
        }
        detail = ""
        if (keys != "") detail = detail "keys: " keys
        if (buckets != "") { if (detail != "") detail = detail "; "; detail = detail "buckets: " buckets }
        escaped_desc = desc
        escaped_detail = detail
        gsub(/\|/, "\\|", escaped_desc)
        gsub(/\|/, "\\|", escaped_detail)
        printf "%s\t%s\t%s\t%s\n", name, type, escaped_desc, escaped_detail
      }
      name = ""; type = ""; desc = ""; buckets = ""; keys = ""
      collecting_desc = 0; desc_buffer = ""
    }

    function append_desc_fragment(fragment) {
      sub(/\\[[:space:]]*$/, "", fragment)
      if (desc_buffer == "") desc_buffer = fragment
      else desc_buffer = desc_buffer " " fragment
    }

    function finish_desc() {
      desc = desc_buffer
      gsub(/[[:space:]]+/, " ", desc)
      sub(/^ /, "", desc)
      sub(/ $/, "", desc)
      collecting_desc = 0
      desc_buffer = ""
    }

    function handle_desc_continuation(line,    closing_pos, remainder) {
      if (!collecting_desc) return 0

      closing_pos = index(line, "\"")
      if (closing_pos > 0) {
        append_desc_fragment(substr(line, 1, closing_pos - 1))
        finish_desc()
        remainder = substr(line, closing_pos + 1)
        if (match(remainder, /vec!\[([0-9.,_ ]+)\]/, b)) {
          buckets = b[1]
          gsub(/[_ ]/, "", buckets)
        }
        if (match(remainder, /&\[([^\]]+)\]/, k)) {
          keys = k[1]
          gsub(/"/, "", keys)
          gsub(/,  */, ", ", keys)
        }
      } else {
        append_desc_fragment(line)
      }

      return 1
    }

    /^\.\/misc\/metrics\// { next }

    handle_desc_continuation($0) { next }

    # ── new ::new( call → flush previous metric and start fresh ──
    match($0, /(Simple|Multi)(Counter|Gauge|Histogram)::new\(/, t) {
      flush_metric()
      type = t[1] t[2]
      # Try to extract name + description + keys + buckets from the same line
      if (match($0, /"(blokli_[a-z0-9_]+)"/, m)) name = m[1]
      if (name != "" && match($0, /blokli_[a-z0-9_]+",[ ]*"([^"]+)"/, d)) desc = d[2]
      if (name != "" && desc == "" && match($0, /blokli_[a-z0-9_]+",[ ]*"([^"]*)$/, d)) {
        collecting_desc = 1
        desc_buffer = ""
        append_desc_fragment(d[1])
      }
      if (name != "" && match($0, /&\[([^\]]+)\]/, k)) {
        keys = k[1]; gsub(/"/, "", keys); gsub(/,  */, ", ", keys)
      }
      if (name != "" && match($0, /vec!\[([0-9.,_ ]+)\]/, b)) {
        buckets = b[1]; gsub(/[_ ]/, "", buckets)
      }
      next
    }

    # ── group separator ──
    /^--$/ {
      flush_metric()
      next
    }

    # ── first metric string is the metric name ──
    name == "" && match($0, /"(blokli_[a-z0-9_]+)"/, m) { name = m[1]; next }

    # ── second quoted string is the description ──
    name != "" && desc == "" && match($0, /"([^"]+)"/, d) { desc = d[1]; next }
    name != "" && desc == "" && match($0, /"([^"]*)$/, d) {
      collecting_desc = 1
      desc_buffer = ""
      append_desc_fragment(d[1])
      next
    }

    # ── collect bucket values (vec![...]) ──
    name != "" && match($0, /vec!\[([0-9.,_ ]+)\]/, b) {
      buckets = b[1]; gsub(/[_ ]/, "", buckets); next
    }

    # ── collect named constant buckets ──
    name != "" && buckets == "" && match($0, /(TIMING_BUCKETS|BUCKETS)/, cb) {
      buckets = cb[1]; next
    }

    # ── collect label keys (&["key1", "key2"]) ──
    name != "" && match($0, /&\[([^\]]+)\]/, k) {
      keys = k[1]; gsub(/"/, "", keys); gsub(/,  */, ", ", keys); next
    }

    END {
      flush_metric()
    }
  ' |
    sort -t$'\t' -k1
}

# ── --generate: print a column-aligned markdown table and exit ────────────────
if [[ ${1:-} == "--generate" ]]; then
  # Collect rows first to compute column widths
  extract_stderr=$(mktemp)
  trap 'rm -f "$extract_stderr"' EXIT
  rows=()
  while IFS=$'\t' read -r name type desc detail; do
    rows+=("$(printf "\`%s\`\t%s\t%s\t%s" "$name" "$type" "$desc" "$detail")")
  done < <(extract_metrics 2>"$extract_stderr")

  if [[ ${#rows[@]} -eq 0 ]]; then
    echo "ERROR: extract_metrics produced no output." >&2
    if [[ -s $extract_stderr ]]; then
      echo "stderr from extract_metrics:" >&2
      cat "$extract_stderr" >&2
    fi
    exit 1
  fi

  # Compute max width per column (start with header widths)
  headers=("Name" "Type" "Description" "Detail")
  widths=(${#headers[0]} ${#headers[1]} ${#headers[2]} ${#headers[3]})
  for row in "${rows[@]}"; do
    IFS=$'\t' read -r c1 c2 c3 c4 <<<"$row"
    ((${#c1} > widths[0])) && widths[0]=${#c1}
    ((${#c2} > widths[1])) && widths[1]=${#c2}
    ((${#c3} > widths[2])) && widths[2]=${#c3}
    ((${#c4} > widths[3])) && widths[3]=${#c4}
  done

  # Print header
  printf "| %-${widths[0]}s | %-${widths[1]}s | %-${widths[2]}s | %-${widths[3]}s |\n" \
    "${headers[0]}" "${headers[1]}" "${headers[2]}" "${headers[3]}"
  # Print separator
  printf "| %s | %s | %s | %s |\n" \
    "$(printf '%0.s-' $(seq 1 ${widths[0]}))" \
    "$(printf '%0.s-' $(seq 1 ${widths[1]}))" \
    "$(printf '%0.s-' $(seq 1 ${widths[2]}))" \
    "$(printf '%0.s-' $(seq 1 ${widths[3]}))"
  # Print rows
  for row in "${rows[@]}"; do
    IFS=$'\t' read -r c1 c2 c3 c4 <<<"$row"
    printf "| %-${widths[0]}s | %-${widths[1]}s | %-${widths[2]}s | %-${widths[3]}s |\n" \
      "$c1" "$c2" "$c3" "$c4"
  done
  exit 0
fi

# ── --fix: regenerate METRICS.md in place and stage it ─────────────────────────
if [[ ${1:-} == "--fix" ]]; then
  expected=$(bash "$0" --generate)
  echo "$expected" >"$METRICS_DOC"
  git -C "$REPO_ROOT" add "$METRICS_DOC"
  echo "METRICS.md updated and staged."
  exit 0
fi

# ── Lint mode ────────────────────────────────────────────────────────────────

if [[ ! -f $METRICS_DOC ]]; then
  echo "ERROR: METRICS.md not found at $METRICS_DOC" >&2
  exit 1
fi

# Normalize a markdown table: collapse runs of whitespace around pipes so that
# column-aligned (prettified) tables compare equal to compact ones.
normalize_table() {
  sed -E 's/[ ]+\|/|/g; s/\|[ ]+/|/g; s/\|-+/|---/g'
}

# Generate expected content and compare with the actual file (whitespace-tolerant)
expected=$(bash "$0" --generate)

if ! diff -q <(echo "$expected" | normalize_table) <(normalize_table <"$METRICS_DOC") >/dev/null 2>&1; then
  echo "ERROR: METRICS.md is out of date. Differences:"
  diff -u <(normalize_table <"$METRICS_DOC") <(echo "$expected" | normalize_table) | head -40
  echo ""
  echo "Run:  $0 --generate > METRICS.md"
  exit 1
fi

count=$(echo "$expected" | tail -n +3 | wc -l | tr -d ' ')
echo "OK: All $count metrics are in sync between code and METRICS.md."
