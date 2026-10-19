#!/bin/sh
# pre_edit_reach.sh: before an edit to a Rust file, what the file touches.
#
# A Claude Code PreToolUse hook (see settings.snippet.json; it is not enabled anywhere). It reads the hook's JSON on
# stdin, takes tool_input.file_path, and runs `custodian run <file> --kind reach` from the file's directory: the
# resident answers when `custodian serve` runs for the project, else the command line runs the reading itself
# (journaled inside an initialized .custodian folder, where the same file over the same code is a reuse; free mode
# elsewhere). It prints the reading's counts (items, files, crates, binaries, the declarations naming those
# binaries) and the binaries the file reaches, then the receipt line. It never blocks the edit: it exits 0.
#
# CUSTODIAN names the program (default: custodian on PATH).

input=$(cat)
file=$(printf '%s' "$input" | python3 -c 'import json, sys
d = json.load(sys.stdin)
print((d.get("tool_input") or {}).get("file_path", ""))' 2>/dev/null)
[ -n "$file" ] || exit 0
case "$file" in
  *.rs) ;;
  *) exit 0 ;;
esac
dir=$(dirname "$file")
[ -d "$dir" ] || exit 0
custodian=${CUSTODIAN:-custodian}
out=$(cd "$dir" && "$custodian" run "$file" --kind reach 2>&1)
printf '%s\n' "$out" | awk '
  /^## binaries/ { show = 1; print "binaries this file reaches:"; next }
  /^## / { show = 0 }
  show && /^- / { print "  " $0 }
  /^(read |decided|reused|answered|free mode|receipt|\[CUST)/ { print }
'
exit 0
