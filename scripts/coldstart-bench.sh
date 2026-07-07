#!/usr/bin/env bash
# Cold-start benchmark: measure agent orientation cost with and without a
# warm damask graph.
#
# Method: run the same task prompt through `claude -p` twice from the repo
# root — once with .damask/ temporarily hidden (cold: hooks silently no-op,
# agent explores from scratch) and once with it present (warm: briefing and
# peek hooks inject graph knowledge). Compare token usage and turn count.
#
# Usage:   scripts/coldstart-bench.sh "<task prompt>" [runs]
# Example: scripts/coldstart-bench.sh "Where is edge ranking computed and what are its signals?" 3
#
# Caveats:
# - Each run costs real API tokens. Default is 1 run per condition.
# - Single-task results are noisy; use 3+ runs and several task prompts
#   before believing a number.
# - The warm condition is only as good as the graph: run it on a repo with
#   meaningful annotations on the code the task touches.

set -euo pipefail

TASK="${1:?usage: coldstart-bench.sh \"<task prompt>\" [runs]}"
RUNS="${2:-1}"

command -v claude >/dev/null 2>&1 || { echo "error: claude CLI not on PATH" >&2; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "error: jq not on PATH" >&2; exit 1; }
[ -d .damask ] || { echo "error: run from a damask-initialized repo root" >&2; exit 1; }

# One claude -p run; prints "input output cache_read turns" on one line.
measure_once() {
    local out
    out=$(claude -p "$TASK" --output-format json 2>/dev/null) || { echo "0 0 0 0"; return; }
    echo "$out" | jq -r '[
        (.usage.input_tokens // 0),
        (.usage.output_tokens // 0),
        (.usage.cache_read_input_tokens // 0),
        (.num_turns // 0)
    ] | join(" ")'
}

# N runs; prints per-run lines then "TOTAL input output cache_read turns".
measure() {
    local label="$1" i line
    local ti=0 to=0 tc=0 tt=0
    for i in $(seq 1 "$RUNS"); do
        line=$(measure_once)
        echo "  $label run $i: $line (input output cache_read turns)"
        ti=$((ti + $(echo "$line" | cut -d' ' -f1)))
        to=$((to + $(echo "$line" | cut -d' ' -f2)))
        tc=$((tc + $(echo "$line" | cut -d' ' -f3)))
        tt=$((tt + $(echo "$line" | cut -d' ' -f4)))
    done
    echo "$((ti / RUNS)) $((to / RUNS)) $((tc / RUNS)) $((tt / RUNS))"
}

echo "Task: $TASK"
echo "Runs per condition: $RUNS"
echo

echo "== COLD (no damask graph) =="
mv .damask .damask.bench-hidden
trap 'mv .damask.bench-hidden .damask 2>/dev/null || true' EXIT
cold=$(measure cold | tail -1)
mv .damask.bench-hidden .damask
trap - EXIT
echo "  mean: $cold"
echo

echo "== WARM (damask graph + hooks) =="
warm=$(measure warm | tail -1)
echo "  mean: $warm"
echo

cold_total=$(( $(echo "$cold" | cut -d' ' -f1) + $(echo "$cold" | cut -d' ' -f3) ))
warm_total=$(( $(echo "$warm" | cut -d' ' -f1) + $(echo "$warm" | cut -d' ' -f3) ))
echo "== SUMMARY (mean per run) =="
printf "  %-22s %12s %12s\n" "" "cold" "warm"
printf "  %-22s %12s %12s\n" "input+cache tokens" "$cold_total" "$warm_total"
printf "  %-22s %12s %12s\n" "output tokens" "$(echo "$cold" | cut -d' ' -f2)" "$(echo "$warm" | cut -d' ' -f2)"
printf "  %-22s %12s %12s\n" "turns" "$(echo "$cold" | cut -d' ' -f4)" "$(echo "$warm" | cut -d' ' -f4)"
if [ "$cold_total" -gt 0 ]; then
    echo
    echo "  context reduction: $(( (cold_total - warm_total) * 100 / cold_total ))%"
fi
