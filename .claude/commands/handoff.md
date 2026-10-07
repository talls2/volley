---
description: Save a handoff note of this session in the repository so another session can continue it
---

Write a handoff note for this session and push it with the volley repository,
so a Claude Code session on another machine (or in the cloud) can pick up where
this one left off.

Extra instructions from the user, if any: $ARGUMENTS

1. **Write the note** following `.claude/handoff/FORMAT.md`. Cover this whole
   session, not only the last request. Fill the front matter from the real
   state (`git rev-parse --short HEAD`, `git branch --show-current`, the date in
   UTC; device `cloud` in a cloud session, else a generic machine name like
   `MacBook`, not a personal one).
2. **Save it** as `.claude/handoff/sessions/<YYYY-MM-DD-HHMM>-<device>-<topic-slug>.md`
   (UTC, lowercase, hyphens). If this session already wrote a note, update that
   file instead of adding another.
3. **Check it** for secrets and personal data: the repository is public.
4. **Commit and push** on the current branch:
   `git add .claude/handoff/sessions && git commit -m "Handoff: <topic>"`, then
   `git pull --rebase` and `git push`.
5. Tell the user the note's path and the one-line summary, and remind them that
   a new session picks it up with `/catchup` (on the same branch, or it finds
   the newest note on any branch).
