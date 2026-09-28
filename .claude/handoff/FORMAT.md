# Handoff note format

A handoff note lets the next Claude Code session, on any machine, pick up where
this one left off. It is a summary for a reader who wasn't there, not a
transcript. Write it in English, in plain prose and short bullets, and keep it
under about 120 lines.

Never include secrets or personal data: no tokens, keys, passwords, email
addresses, or private file contents. Refer to local paths only by their place in
the repository.

```markdown
---
date: 2026-09-28 15:30 UTC
device: MacBook          # or "cloud", or the machine's name
branch: main             # the volley branch the work is on
commit: 8383677          # volley HEAD when the note was written (short sha)
topic: Second hero, Nova  # a few words
---

# Nova, the second hero

## Where things stand
What exists now, in a few sentences: what was built, what works, what's pushed
and where (branch, commit, PR), and anything left only on this machine
(uncommitted changes, files outside the repository such as downloads or
Blender files).

## Decisions
- Each decision the user made or agreed to, and why. These are what a new
  session most needs, since they aren't in the code.

## Next steps
1. What the user wants done next, most important first.

## Open questions
- Anything undecided, or that the user said they'd come back to.

## Context
Anything else the next session should know: the user's preferences for this
work, ideas they liked or rejected, references (concept art, links).
```

Leave out a section that would be empty.
