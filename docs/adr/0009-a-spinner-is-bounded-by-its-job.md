# A spinner is bounded by its job

AGENTS.md states the rule this file makes an exception to: **draw only when something changed**, and
idle CPU is 0%. The rule is not a preference. A frame costs milliseconds, and drawing one per event
rather than one per batch queued 2.3 seconds of stale frames behind a trackpad flick — 200 wheel
events, 253 draws — which read as the TUI freezing. Every redraw in Varde is caused by a key, a mouse
event, pty output, a watcher event, or a git status that actually differs.

Risk analysis is the first work Varde does that takes long enough for the user to wonder whether it
is doing anything, and the first compute it runs off the main loop at all. Until now the only thing
that arrived asynchronously was the `notify` watcher's channel, drained once per poll — OS-driven,
never Varde's own work. So both halves of this are new: a spawned thread that computes, and a screen
that has to say it is computing.

**The decision: a job in flight is a redraw source, and nothing else about the rule changes.** While
an analysis or a Refactor loop is running, the edge redraws on a tick so the spinner advances. When no
job is in flight, redraws are input-driven exactly as before and idle CPU returns to 0%. The exception
is bounded by construction rather than by discipline — there is no state in which Varde spins with
nothing to spin for, because the tick exists only as long as the job does.

**Static text was the honest alternative and it is the one worth naming.** `computing…` on the tree
border, replaced when the completion Event arrives, costs zero redraws and bends no rule. It was
turned down because a long analysis then looks identical to a hang, and the whole reason the figure is
computed at startup rather than on demand is that nobody should have to ask whether it is happening.

**Advancing the spinner on incidental frames** — only when a key or a mouse event already caused a
draw — was rejected as the worse lie of the three. It stalls precisely when the user stops touching
anything, which is exactly when they are watching it, and a stalled spinner claims the job died.

**Progress events were rejected for the reason the batching rule exists.** A thread reporting files
scanned floods the main loop, and every message is arguably a frame: that is the 253-draws measurement
again, arrived at from a different direction. The job reports **completion only**. If analysis of a
large workspace turns out to be slow enough to want a percentage, it can be added as a rate-limited
event with a measurement behind it, which is the standard this repo already holds itself to.

## Consequences

**The tick is the edge's, and the decision is not.** `update` returns an Effect describing work to be
done; the edge spawns the thread, drains its channel the way it drains the watcher's, and queues an
Event when it finishes. The core never learns that a thread exists — same shape as every other fact
only the edge can observe.

**One frame per batch still holds.** The tick is another input to `pump`, not a bypass of it. A tick
that arrives alongside a hundred wheel events produces one frame, not a hundred and one.

**A job that never finishes spins forever, and that is deliberate.** The Refactor loop waits for the
AI session with no timeout (`docs/adr/0010-varde-owns-the-test-gate.md`), so an indefinite spin is a
reachable state. It is only survivable because the pane says what is being waited for: a spinner with
no caption is indistinguishable from a hang, which is the failure this whole file is about.

**A recompute supersedes; a loop refuses.** Two analyses of two different workspace states cannot both
be interesting — the newer request is the only one whose answer will still be true, so it cancels the
older. Two Refactor loops editing the same files at once is a different thing entirely, and the second
is refused rather than queued.

**A playing Reading is the same exception, widened once and deliberately.** Reading a Selection aloud
(F35) needs the core to know where the sound has got to — `:next` and `:prev` seek from the Utterance
being spoken, and a pause records the offset resuming rewrites from — so the edge queues
`Event::Speaking` while, and only while, it holds a player. That is this file's decision with a
different child: bounded by construction rather than by discipline, on the same 80ms cadence, and
gone the moment the sound is. It is named here rather than left implicit because the rule reads "a
job in flight" and a player is not a job — the shape is what generalises, not the word.

**A held drag is the same exception a third time, and the widening is the same shape.** A drag held
at or past a pane's edge has to keep scrolling while the pointer stays still, and a pointer that is
not moving is a pointer the terminal reports nothing about — so without a cadence there is nothing to
move it, and a drag parked one row past the border sits there doing nothing. The edge reports the
held drag again while, and only while, it is holding one: `mouse::Pointer::held` names the report,
`mouse::dragged` clears it on every drag that is not pushing and on the release, and the core decides
what the step is. Bounded twice over, in fact — the button ends it, and so does the text, because the
step at the end of a buffer lands where the drag already is and nothing is held for a view that
cannot move. Named here for the reason the Reading is: the rule reads "a job in flight", and a drag
is no more a job than a player is.

**The tree filter's walk and find stream their answers, and that narrows the progress rule rather
than breaking it (#109).** Find over tens of thousands of files is slow enough that waiting for
completion leaves the box empty for the whole search, and the issue asked for hits to appear as they
are found. What the progress rule turned down was a message per file, each one arguably a frame. The
edge still sends no such message. It drains whatever a job has produced since the last pass of the
loop into one `Event::Indexed` or `Event::Searched`, so a job adds at most one event per pass. That
event is batched with the input like every other, and the frame count is the one the batching rule
already allows. Both jobs spin while, and only while, they are in flight: the walk until its walker
is done, and the search until it is done, cut short at the cap, or superseded by a newer query.

**Never parse per frame still holds too, and harder.** The analysis is cached against the commit it
was computed at, and a save marks the figure stale rather than starting a job. Re-analysing a
workspace per keystroke-batch is the failure mode this rule was written for, and a figure that churns
while you type is noise rather than signal.
