---
description: Save a handoff note of this session to talls2/volley-notes so another session can continue it
---

Write a handoff note for this session and push it to the private repository
`talls2/volley-notes`, so a Claude Code session on another machine can pick up
where this one left off.

Extra instructions from the user, if any: $ARGUMENTS

1. **Find the notes repository.**
   - Locally: use `$VOLLEY_NOTES_DIR` if set, else `~/volley-notes`. If it isn't
     there, clone it: `git clone https://github.com/talls2/volley-notes ~/volley-notes`.
   - In a cloud session (`$CLAUDE_CODE_REMOTE` is set): attach it with the
     `add_repo` tool (owner `talls2`, repo `volley-notes`, access `push`), clone
     it next to the volley checkout as its result says, and register it.
   - Then `git pull --rebase` in it. If it can't be reached, say so, show the
     note in chat instead, and stop.
2. **Write the note** following `.claude/handoff/FORMAT.md` in the volley
   repository. Cover this whole session, not only the last request. Fill the
   front matter from the real state (`git rev-parse --short HEAD`,
   `git branch --show-current`, the date in UTC; device `cloud` in a cloud
   session, else the machine's name).
3. **Save it** as `sessions/<YYYY-MM-DD-HHMM>-<device>-<topic-slug>.md` (UTC,
   lowercase, hyphens) in the notes repository. If this session already wrote a
   note, update that file instead of adding another.
4. **Check it** for secrets and personal data before committing (see the format).
5. **Commit and push**: `git add sessions && git commit -m "Handoff: <topic>"`,
   then `git push`; if the push is rejected, `git pull --rebase` and push again.
6. Tell the user the note's path and the one-line summary, and remind them that
   a new session picks it up with `/catchup`.
