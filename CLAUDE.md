# Volley

See README.md for what the game is, how it's laid out and how to run it.

## Source assets

The tools' source files (Mixamo, mocap, videos, raw hero models) are in the
private `talls2/volley-assets`, cloned empty at `~/Downloads/volley-assets`.
Disk is tight: check out only the folders a task needs (`git sparse-checkout
add mixamo mocap` to retarget, `models` to rig a hero), and when done, delete
the clone and clone it empty again. New downloads get committed there, never
here. Its README has the commands.

## Continuing work across sessions

Work on volley happens in several Claude Code sessions, on different machines
(a MacBook, cloud sessions). They hand off through notes kept in this
repository, in `.claude/handoff/sessions/` (one note per session, named by
date), committed on the branch the work is on.

- **`/catchup`** reads the latest notes (on any branch) and continues from them.
  When the user refers to work you have no record of ("the other session", "the
  character we were building"), run it before saying you don't know.
- **`/handoff`** writes a note of the current session and pushes it. Offer it
  when the user says they're switching machines or stopping for now.
- **Keep the session's note current**: after each milestone, update it (where
  things stand, and above all what's coming next), commit and push it with the
  work. The user asked for this so a new session can always pick up.

Notes are summaries, never transcripts, and never contain secrets or personal
data (the repository is public); `.claude/handoff/FORMAT.md` has the format.
