---
description: Read the latest handoff notes and continue from them
---

Catch up on work done in other Claude Code sessions on volley, then continue
from it.

Extra instructions from the user, if any: $ARGUMENTS

1. `git fetch --all`. Notes live in `.claude/handoff/sessions/` on whichever
   branch the work was on: find the newest across branches, e.g.
   `git log --all --format='%h %D %ci' -- .claude/handoff/sessions | head`, and
   read them with `git show <branch>:<path>` (or check that branch out if the
   user wants to continue there).
2. Read the most recent notes (file names sort by date): the latest three, or
   those from the last two days if there are more. If the user named a topic,
   read the notes about it instead.
3. Check them against the code: look at the branches and commits they mention,
   since work may have moved on since.
4. Tell the user briefly where things stand, what's next, and anything the
   notes left open or that no longer matches the code. Then carry on with the
   next step if the user asked you to continue, keeping the note current as
   you go (CLAUDE.md).
