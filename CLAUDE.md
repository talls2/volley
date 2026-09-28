# Volley

See README.md for what the game is, how it's laid out and how to run it.

## Continuing work across sessions

Work on volley happens in several Claude Code sessions, on different machines
(a MacBook, cloud sessions). They hand off through handoff notes in the private
repository `talls2/volley-notes` (`sessions/`, one note per session, named by
date). Only the owner has access; for anyone else, ignore this section.

- **`/catchup`** reads the latest notes and continues from them. When the user
  refers to work you have no record of ("the other session", "the character we
  were building"), run it before saying you don't know.
- **`/handoff`** writes a note of the current session and pushes it. Offer it
  when the user says they're switching machines or stopping for now, and after
  a milestone in a long session. In cloud sessions it's the only way notes get
  written, so offer it before the session ends.
- Local sessions also write a note automatically when they end
  (`.claude/hooks/handoff-on-exit.sh`), if `~/volley-notes` is cloned.

Notes are summaries, never transcripts, and never contain secrets or personal
data; `.claude/handoff/FORMAT.md` has the format.
