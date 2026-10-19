#!/bin/sh
# pre_edit_reach.sh: before an edit, may the file be edited by hand, and what it touches.
#
# A Claude Code PreToolUse hook (see settings.snippet.json; it is not enabled anywhere). It reads the hook's JSON on
# stdin, takes tool_input.file_path, and from the file's directory:
#
#   1. asks `custodian agent guard <file>`: in a bounded job, code is written through change specs (preflight, then
#      transform), so a hand edit of code is refused; the guard's words go to stderr and the hook exits 2, which
#      blocks the edit and tells the agent why and what to run instead. Anywhere else the guard says yes.
#   2. from `custodian agent brief --head`, prints its one line (the mode, the decisions open, the jobs not
#      completed) and the decisions to make first.
#   3. for a Rust file, runs `custodian run <file> --kind reach`: the resident answers when `custodian serve` runs
#      for the project, else the command line runs the reading itself (journaled inside an initialized .custodian
#      folder, where the same file over the same code is a reuse; free mode elsewhere). It prints the binaries the
#      file reaches, the entries naming them, the env keys its items read (custodian_kit's `reach-env` rows: the
#      keys the registry marks `reads_env`), then the receipt line.
#
# Only the guard blocks: every other step exits 0 whatever it found.
#
# CUSTODIAN names the program (default: custodian on PATH).

input=$(cat)
file=$(printf '%s' "$input" | python3 -c 'import json, sys
d = json.load(sys.stdin)
print((d.get("tool_input") or {}).get("file_path", ""))' 2>/dev/null)
[ -n "$file" ] || exit 0
dir=$(dirname "$file")
[ -d "$dir" ] || exit 0
custodian=${CUSTODIAN:-custodian}

guard=$(cd "$dir" && "$custodian" agent guard "$file" 2>&1)
if [ $? -eq 2 ]; then
  printf '%s\n' "$guard" | grep -v '^receipt #' >&2
  exit 2
fi

(cd "$dir" && "$custodian" agent brief --head 2>/dev/null) | awk '
  /^## decisions to make first/ { show = 1; print "decisions to make first:"; next }
  /^## / { show = 0 }
  show && /^- / { print "  " $0 }
  /^mode: .* · / { print }
'

case "$file" in
  *.rs) ;;
  *) exit 0 ;;
esac
out=$(cd "$dir" && "$custodian" run "$file" --kind reach 2>&1)
printf '%s\n' "$out" | awk '
  /^## binaries/ { show = 1; print "binaries this file reaches:"; next }
  /^## reach-entry/ { show = 1; print "entries naming those binaries:"; next }
  /^## reach-env/ { show = 1; print "env keys its items read:"; next }
  /^## / { show = 0 }
  show && /^- / { print "  " $0 }
  /^(read |decided|reused|answered|free mode|receipt|\[CUST)/ { print }
'
exit 0
