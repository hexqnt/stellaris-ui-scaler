#!/usr/bin/env bash
set -euo pipefail

if [[ ${1-} == --help ]]; then
    cat <<'EOF'
Usage: tools/perf.sh [--] [application arguments...]

Build the profiling profile and record CPU samples with DWARF call stacks.
Without arguments, launch the GUI. All arguments are passed to the application.
Run from the repository root to use its profiles and AI resources.

Environment:
  PERF_OUTPUT_DIR  Parent directory for recordings (default: <repo>/target/perf)
  PERF_FREQ        Sampling frequency in Hz (default: 199)

Examples:
  tools/perf.sh
  tools/perf.sh build --game /path/to/Stellaris --profile profiles/4k.toml --output output/profiled-mod
  tools/perf.sh -- --help
EOF
    exit 0
fi
if [[ ${1-} == -- ]]; then
    shift
fi

fail() {
    printf 'Error: %s\n' "$*" >&2
    exit 1
}

[[ $(uname -s) == Linux ]] || fail 'This script requires Linux.'
for tool in cargo rustc perf; do
    command -v "$tool" >/dev/null || fail "Required command not found: $tool"
done

frequency=${PERF_FREQ:-199}
[[ $frequency =~ ^[1-9][0-9]*$ ]] || fail 'PERF_FREQ must be a positive integer.'
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)

# Проверяем доступ до сборки; настройки ядра скрипт не меняет.
if ! perf stat -e cpu-clock:u -- true; then
    fail 'Cannot access perf events. Check the perf diagnostic above and kernel.perf_event_paranoid.'
fi

# Явная native-цель исключает случайную кросс-компиляцию через конфигурацию Cargo.
host=$(cd -- "$root" && rustc -vV | sed -n 's/^host: //p')
[[ -n $host ]] || fail 'Cannot determine the Rust host target.'
(
    cd -- "$root"
    cargo build --locked --profile profiling --bin stellaris-ui-scaler \
        --target "$host" --target-dir "$root/target"
)

output_parent=${PERF_OUTPUT_DIR:-"$root/target/perf"}
mkdir -p -- "$output_parent"
output_parent=$(cd -- "$output_parent" && pwd)
output=$(mktemp -d "$output_parent/$(date -u +%Y%m%dT%H%M%SZ)-XXXXXX")
binary="$output/stellaris-ui-scaler"
# Копия сохраняет символы именно этой записи после следующих сборок проекта.
cp -- "$root/target/$host/profiling/stellaris-ui-scaler" "$binary"

record=(perf record -e cpu-clock:u -F "$frequency" --call-graph dwarf,32768
    -o "$output/perf.data" -- "$binary" "$@")
{
    printf 'Recorded at: %s\nWorking directory: %s\n' "$(date -u --iso-8601=seconds)" "$PWD"
    printf 'Command: '
    printf '%q ' "${record[@]}"
    printf '\n'
    (cd -- "$root" && rustc -Vv && cargo -V)
    perf version
    uname -a
    printf 'RAYON_NUM_THREADS: %s\n' "${RAYON_NUM_THREADS:-unset}"
    if command -v lscpu >/dev/null; then
        lscpu
    fi
    if git -C "$root" rev-parse HEAD; then
        git -C "$root" status --short
        git -C "$root" diff HEAD -- > "$output/source.diff"
    fi
} > "$output/run.txt" 2>&1

printf 'Recording to: %s\nPerform the workload, then close the application or press Ctrl+C.\n' "$output"
# После Ctrl+C даём perf завершить запись и формируем отчёты.
trap ':' INT
status=0
if "${record[@]}" 2>&1 | tee "$output/run.log"; then
    status=0
else
    status=${PIPESTATUS[0]}
fi
printf 'perf record exit status: %s\n' "$status" >> "$output/run.txt"

if [[ -s $output/perf.data ]]; then
    if ! perf report --stdio --no-children --percent-limit 0 -i "$output/perf.data" \
        > "$output/report-self.txt" 2> "$output/report-self.log"; then
        printf 'Warning: self-time report failed; see report-self.log.\n' >&2
    fi
    if ! perf report --stdio --children --percent-limit 0 -i "$output/perf.data" \
        > "$output/report-children.txt" 2> "$output/report-children.log"; then
        printf 'Warning: children report failed; see report-children.log.\n' >&2
    fi
else
    printf 'Error: perf did not produce a nonempty recording.\n' >&2
    (( status != 0 )) || status=1
fi

printf 'Profile directory: %s\nOpen interactively: ' "$output"
printf 'perf report -i %q\n' "$output/perf.data"
exit "$status"
