#!/usr/bin/env bash
# SessionEnd hook: when a local Claude Code session on volley ends, have Claude
# write a handoff note of it (.claude/handoff/FORMAT.md) and push it to the
# private notes repository, so a session on another machine can /catchup.
#
# Does nothing unless the notes repository is cloned (VOLLEY_NOTES_DIR, default
# ~/volley-notes), so it's harmless for anyone else working on volley. Skips
# cloud sessions (they use /handoff), sessions that already ran /handoff, and
# sessions with fewer than two messages. The note is written in the background,
# so quitting isn't held up; its log is ~/.claude/volley-handoff.log.

set -u

[ -n "${VOLLEY_HANDOFF_CHILD:-}" ] && exit 0 # the summarizing session itself
[ -n "${CLAUDE_CODE_REMOTE:-}" ] && exit 0
notes="${VOLLEY_NOTES_DIR:-$HOME/volley-notes}"
[ -d "$notes/.git" ] || exit 0
command -v jq >/dev/null && command -v claude >/dev/null && command -v git >/dev/null || exit 0

input=$(cat)
transcript=$(jq -r '.transcript_path // empty' <<<"$input")
[ -f "$transcript" ] || exit 0
grep -q '<command-name>/handoff</command-name>' "$transcript" && exit 0

project="${CLAUDE_PROJECT_DIR:-$(pwd)}"
format="$project/.claude/handoff/FORMAT.md"
[ -f "$format" ] || exit 0

# The conversation's text, without tool calls and results; the most recent part
# if it's very long.
conversation=$(jq -r '
  select((.type == "user" or .type == "assistant") and .isMeta != true and .isSidechain != true)
  | .message as $m
  | ($m.content | if type == "string" then . else [.[]? | select(.type == "text") | .text] | join("\n") end) as $t
  | select($t != "")
  | "\n### \($m.role)\n\($t)"' "$transcript" 2>/dev/null | tail -c 200000)
turns=$(grep -c '^### user$' <<<"$conversation")
[ "$turns" -ge 2 ] || exit 0

branch=$(git -C "$project" branch --show-current 2>/dev/null)
commit=$(git -C "$project" rev-parse --short HEAD 2>/dev/null)
device=$(scutil --get ComputerName 2>/dev/null || hostname -s)
now=$(date -u '+%Y-%m-%d %H:%M UTC')
stamp=$(date -u '+%Y-%m-%d-%H%M')

write_note() {
	cd "$notes" || return 1

	local prompt note topic slug device_slug file
	prompt="Below is a Claude Code conversation about the volley game (github.com/talls2/volley).
Write its handoff note, following this format exactly:

$(cat "$format")

Front matter: date: $now, device: $device, branch: ${branch:-unknown}, commit: ${commit:-unknown}.
Output only the note, starting with the --- line. If the conversation has nothing
worth handing off (a quick question, nothing decided or built), output only SKIP."

	note=$(printf '%s\n' "$conversation" | VOLLEY_HANDOFF_CHILD=1 claude -p --model sonnet "$prompt") || return 1
	note=$(sed -e '/^```/d' <<<"$note")
	[ "$(tr -d '[:space:]' <<<"$note")" = SKIP ] && { echo "$now: nothing to hand off"; return 0; }
	grep -q '^---$' <<<"$note" || { echo "$now: unexpected output, not saved"; return 1; }

	topic=$(sed -n 's/^topic:[[:space:]]*//p' <<<"$note" | head -1)
	slug=$(tr '[:upper:]' '[:lower:]' <<<"${topic:-session}" | sed -e 's/[^a-z0-9]\{1,\}/-/g' -e 's/^-//' -e 's/-$//' | cut -c1-40)
	device_slug=$(tr '[:upper:]' '[:lower:]' <<<"$device" | sed -e 's/[^a-z0-9]\{1,\}/-/g' -e 's/^-//' -e 's/-$//')
	file="sessions/$stamp-$device_slug-$slug.md"

	git pull --rebase -q
	mkdir -p sessions
	printf '%s\n' "$note" >"$file"
	git add "$file"
	git commit -q -m "Handoff: ${topic:-session} ($device)"
	git push -q || { git pull --rebase -q && git push -q; }
	echo "$now: saved $file"
}

mkdir -p "$HOME/.claude"
nohup bash -c "$(declare -f write_note); $(declare -p notes format conversation branch commit device now stamp); write_note" \
	>>"$HOME/.claude/volley-handoff.log" 2>&1 </dev/null &
disown
exit 0
