#!/usr/bin/env bash

# Compares pairs of benchmark JSON files and exits non-zero on >20% regression.
# Usage: bash scripts/check-bench-regression.sh BASE1 CUR1 [BASE2 CUR2 ...]
# JSON schema: { bench_id, metric, unit, p50, p95, n, timestamp, notes? }

set -euo pipefail

if (( $# < 2 || ($# % 2) != 0 )); then
  echo "Usage: $0 BASELINE.json CURRENT.json [BASE2.json CUR2.json ...]" >&2
  exit 2
fi

missing=()
for f in "$@"; do
  if [[ ! -f "$f" ]]; then missing+=("$f"); fi
done
if (( ${#missing[@]} > 0 )); then
  echo "Missing file(s): ${missing[*]}" >&2
  exit 3
fi

extract_num() {
  # Greedy match for numeric JSON value by key (simple, controlled files)
  local key="$1" file="$2"
  grep -oE '"'"$key"'"[[:space:]]*:[[:space:]]*[-]?[0-9]+(\.[0-9]+)?' "$file" | head -n1 | sed -E 's/.*:[[:space:]]*//' || true
}

extract_str() {
  local key="$1" file="$2"
  grep -oE '"'"$key"'"[[:space:]]*:[[:space:]]*"[^"]*"' "$file" | head -n1 | sed -E 's/.*:[[:space:]]*"(.*)"/\1/' || true
}

validate_json_schema() {
  # Validate required keys and value types: bench_id, metric, unit (string); p50,p95 (number); n (integer >=1); timestamp (string)
  local file="$1"; local ok=1
  local bid metric unit p50 p95 n ts
  bid=$(extract_str bench_id "$file"); metric=$(extract_str metric "$file"); unit=$(extract_str unit "$file");
  p50=$(extract_num p50 "$file"); p95=$(extract_num p95 "$file"); n=$(grep -oE '"n"[[:space:]]*:[[:space:]]*[0-9]+' "$file" | head -n1 | sed -E 's/.*:[[:space:]]*//');
  ts=$(extract_str timestamp "$file")
  if [[ -z "$bid" || -z "$metric" || -z "$unit" || -z "$p50" || -z "$p95" || -z "$n" || -z "$ts" ]]; then ok=0; fi
  if (( ${n:-0} < 1 )); then ok=0; fi
  # p95 >= p50 check (basic sanity)
  ge=$(awk -v a="$p95" -v b="$p50" 'BEGIN{print (a+0>=b+0)?1:0}')
  if (( ge != 1 )); then ok=0; fi
  return $(( ok==1 ? 0 : 1 ))
}

worst=0
declare -i failures=0

pair_index=0
while (( $# >= 2 )); do
  base="$1"; cur="$2"; shift 2; ((pair_index++))
  # Schema validation first
  if ! validate_json_schema "$base"; then
    echo "[schema] Invalid baseline JSON: $base" >&2
    failures+=1
    continue
  fi
  if ! validate_json_schema "$cur"; then
    echo "[schema] Invalid current JSON: $cur" >&2
    failures+=1
    continue
  fi
  bench_id_b=$(extract_str bench_id "$base")
  bench_id_c=$(extract_str bench_id "$cur")
  bench_id=${bench_id_c:-$bench_id_b}
  metric=$(extract_str metric "$cur")
  unit=$(extract_str unit "$cur")
  p95_base=$(extract_num p95 "$base")
  p95_cur=$(extract_num p95 "$cur")

  if [[ -z "$metric" || -z "$p95_base" || -z "$p95_cur" ]]; then
    echo "[$bench_id] Invalid or missing keys (metric/p95) in files: $base $cur" >&2
    failures+=1
    continue
  fi

  # Determine direction: for duration, lower is better; for throughput, higher is better.
  # Default to duration-like if unknown.
  direction="duration" # or "throughput"
  if [[ "$metric" == "throughput_mbs" || "$unit" =~ MB/?s ]]; then direction="throughput"; fi

  regression=0
  diff_pct=0
  if [[ "$direction" == "duration" ]]; then
    # worse if current higher
    cmp=$(awk -v c="$p95_cur" -v b="$p95_base" 'BEGIN{print (c>b)?1:0}')
    if (( cmp == 1 )); then
      diff_frac=$(awk -v c="$p95_cur" -v b="$p95_base" 'BEGIN{ if (b==0) print 0; else printf "%.10f", (c-b)/b }')
      diff_pct=$(awk -v f="$diff_frac" 'BEGIN{ printf "%.2f", f*100 }')
      gt=$(awk -v f="$diff_frac" 'BEGIN{print (f>0.20)?1:0}')
      if (( gt == 1 )); then regression=1; fi
    else
      diff_pct=$(awk -v c="$p95_cur" -v b="$p95_base" 'BEGIN{ if (b==0) print 0; else printf "%.2f", (b-c)/b*100 }')
    fi
  else
    # throughput: worse if current lower
    cmp=$(awk -v c="$p95_cur" -v b="$p95_base" 'BEGIN{print (c<b)?1:0}')
    if (( cmp == 1 )); then
      diff_frac=$(awk -v c="$p95_cur" -v b="$p95_base" 'BEGIN{ if (b==0) print 0; else printf "%.10f", (b-c)/b }')
      diff_pct=$(awk -v f="$diff_frac" 'BEGIN{ printf "%.2f", f*100 }')
      gt=$(awk -v f="$diff_frac" 'BEGIN{print (f>0.20)?1:0}')
      if (( gt == 1 )); then regression=1; fi
    else
      diff_pct=$(awk -v c="$p95_cur" -v b="$p95_base" 'BEGIN{ if (b==0) print 0; else printf "%.2f", (c-b)/b*100 }')
    fi
  fi

  # Track worst absolute percentage difference (for summary)
  abs_pct=$(echo "$diff_pct" | sed 's/^-//')
  gt_worst=$(awk -v a="$abs_pct" -v b="$worst" 'BEGIN{print (a>b)?1:0}')
  if (( gt_worst == 1 )); then worst=$abs_pct; fi

  if (( regression == 1 )); then
  echo "::warning::[$bench_id] p95 ${metric} regressed by ${diff_pct}% (baseline=$p95_base -> current=$p95_cur)"
    failures+=1
  else
  echo "[OK][$bench_id] p95 ${metric} change ${diff_pct}% (baseline=$p95_base, current=$p95_cur)"
  fi
done

if (( failures > 0 )); then
  echo "Overall: $failures regression(s) detected; worst delta $(printf '%.2f' "$worst")%" >&2
  exit 1
else
  echo "Overall: no regressions; worst delta $(printf '%.2f' "$worst")%"
fi
