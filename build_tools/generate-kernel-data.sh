#!/usr/bin/env sh
set -eu

script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd -P)
repo_root=$(CDPATH= cd "$script_dir/.." && pwd -P)
cd "$repo_root"

log() {
    printf '%s\n' "$*" >&2
}

fail() {
    log "error: $*"
    exit 1
}

need_command() {
    command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

kernel_binary_name() {
    case "$(uname -s 2>/dev/null || printf unknown)" in
        MINGW*|MSYS*|CYGWIN*) printf 'WolframKernel.exe' ;;
        *) printf 'WolframKernel' ;;
    esac
}

find_kernel() {
    if [ -n "${WOLFRAM_KERNEL:-}" ]; then
        printf '%s\n' "$WOLFRAM_KERNEL"
        return 0
    fi

    kernel_name=$(kernel_binary_name)
    if command -v wolframscript >/dev/null 2>&1; then
        kernels_tmp=$(mktemp "${TMPDIR:-/tmp}/wolfsh-kernels.XXXXXX")
        if wolframscript -showkernels > "$kernels_tmp" 2>/dev/null; then
            while IFS= read -r line; do
                line=$(printf '%s\n' "$line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
                [ -n "$line" ] || continue
                case "$line" in
                    *"/$kernel_name"|*"\\$kernel_name")
                        if [ -x "$line" ] || [ -f "$line" ]; then
                            rm -f "$kernels_tmp"
                            printf '%s\n' "$line"
                            return 0
                        fi
                        ;;
                esac
            done < "$kernels_tmp"
        fi
        rm -f "$kernels_tmp"
    fi

    if command -v "$kernel_name" >/dev/null 2>&1; then
        command -v "$kernel_name"
        return 0
    fi

    return 1
}

validate_builtin_symbols() {
    awk -F '\t' '
        NF == 2 && $1 != "" && $2 ~ /^[0-9]+$/ {
            count++
            if ($1 == "Plot") has_plot = 1
        }
        END { exit !(count > 1000 && has_plot) }
    ' "$1"
}

need_command awk
kernel=$(find_kernel) || fail "could not find WolframKernel. Set WOLFRAM_KERNEL to the kernel executable path."

output="$script_dir/builtin_symbols.tsv"
tmp=$(mktemp "${TMPDIR:-/tmp}/wolfsh-builtin-symbols.XXXXXX")
trap 'rm -f "$tmp"' EXIT HUP INT TERM

log "Generating $output"
log "Using kernel: $kernel"

query='(Get["build_tools/wl/query_to_output_form.wl"])[(Get["build_tools/wl/builtin_symbols.wl"])[]]'
if ! "$kernel" -noprompt -run "$query" > "$tmp"; then
    fail "WolframKernel failed while generating builtin symbols"
fi

if ! validate_builtin_symbols "$tmp"; then
    log "Generated output did not look like a valid builtin symbol table. First lines:"
    sed -n '1,20p' "$tmp" >&2
    fail "refusing to replace $output"
fi

mv "$tmp" "$output"
trap - EXIT HUP INT TERM

lines=$(wc -l < "$output" | tr -d ' ')
log "Wrote $output ($lines lines)"
