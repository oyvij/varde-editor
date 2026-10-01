# Slow work runs off the loop and answers as data

The main loop reads a key, runs it through `update`, and draws, and anything else it does in the
same iteration is time that key waits. #86 measured what that costs: stepping the file tree felt
slow, and the step itself was nearly free. The key waited on two things that had nothing to do with
it. Every two seconds the git poll diffed HEAD against the working tree on the loop, 40–78 ms that
any key landing on it waited through (#98). And every event paid 5–6 ms copying `State`, most of it
Authorship, which held two strings for every line of every open buffer.

Some work already ran off the loop: risk analysis, the test gate, the formatter, the release check,
the binary replace, the first parse of a reopened file and its blame (#83). Each was moved when
someone felt it, and none of them was moved because of a rule. This ADR states the rule, so the next
feature is built off the loop from the start rather than moved there after somebody notices.

**The decision: work whose cost grows with the input runs on a thread, and its answer comes back as
data.** Anything that touches the disk, git, a process or the network, or that scales with the size
of a file or of the repository, is not done on the loop. What stays on the loop is work whose cost
does not grow with the input: an `update`, a draw, a channel drained.

## The two shapes

Effects as data are what make this cheap. `update` never performs work, it only asks for it, so where
that work runs is the edge's choice alone, and the core does not change when the edge moves it.

**Work the core asks for.** `update` returns an `Effect`; the edge spawns a thread, moves a snapshot
of the inputs into it, and the thread's answer comes back as an `Event` that `update` decides on.
`AnalyseRisk`, `RunTests`, `RunFormatter`, `CheckRelease` and `ReplaceBinary` are this shape. The
answer carries what it was asked about, such as a `generation`, a buffer's `revision` or a commit, so
`update` can tell a stale answer from a current one and drop it.

**Facts only the edge can observe.** The syntax parse, Authorship and the git poll answer into a
cache the edge holds, and the edge tells the core, the way it tells the core about the hosted panes.
They never become an `Event`, because `update` reads those fields and never writes them (AGENTS.md,
*A fact only the edge can observe is told, never remembered*). An in-flight set, like the one blame
keeps, stops the same question being asked twice.

## The rules

- **The thread owns what it reads.** It gets a copy of its inputs, never `State` behind a lock. A
  lock around `State` is a second writer to the core, which the rule of one `update` forbids.
- **The thread decides nothing.** It reads and converts. What an answer means is the library's to
  decide, under a test, as it is for every other input.
- **At most one job in flight per question.** A poll that falls due while the last one is still out
  is skipped, not queued behind it. A queue of stale work is a backlog, and a backlog reads as a
  frozen TUI in exactly the way the batching rule exists to prevent.
- **An answer redraws only when it differs.** An answer is a redraw source like pty output, so idle
  CPU stays at 0%. A spinner may turn only while its job is in flight
  (`docs/adr/0009-a-spinner-is-bounded-by-its-job.md`).

## Large read-only state is shared, not owned

The other half of #86 was on the loop itself. `update` takes `&State` and returns a new one, so every
field is cloned on every event and the old copy is dropped. A field is paid for on every keypress in
proportion to its size, whether or not the event touched it. A large field the core only reads, which
is what the edge's observations usually are, is held behind an `Arc` so the clone costs a reference
count. Authorship became `Arc<[Authored]>` per file for this reason, and a bare `State` clone went
from 3.4 ms to about 1.7 ms.

This is not a licence to share mutable state. An `Arc` holds what the edge told and nobody edits in
place. Anything `update` changes stays owned, so the one-function rule still holds.

## Considered and rejected

**An async runtime.** Tokio would give the jobs a scheduler, but the loop is synchronous by design,
it polls crossterm at 16 ms and drains a batch, and every job it has run so far needed one thread
and one channel. `docs/stack.md` already holds tokio to a dev-dependency, and the LSP framing crate
was chosen because it brings no runtime. A runtime is a second concurrency model for work that plain
threads already do.

**A worker pool.** No job has yet been frequent enough for thread creation to show in a measurement.
"One in flight per question" already bounds how many threads exist at once. A pool is an answer to a
problem nobody has measured.

**One job channel.** The five jobs the core asks for each have a typed channel, a `Sender` on the
edge, a `Receiver`, and a line in `collect_job_events` that turns the answer into its `Event`. A
single `Sender<Event>`, with each thread sending its finished `Event`, would make a new job one
`Event` variant and one spawn. It was turned down because the typed channels read better. Each
channel's type states what its job answers, and `collect_job_events` shows every answer becoming
its `Event` in one place, where a single channel would spread that across five spawn sites. A new
job pays for a channel and one conversion line, and in exchange the whole set is readable in one
function.

**Leaving work on the loop until it is felt.** This was the practice until now, and it is how a git
poll that was cheap on a clean tree cost up to 78 ms on a branch with large uncommitted files. It is
felt first by the user, on the input they happen to have, and a feature's own tests never feel it.

## Consequences

- A feature that reads files, git, a process or the network ships with its work off the loop. The
  code review asks where its work runs, not only what it does.
- Synchronous effects remain, such as reading stories and branches, and checking out a branch.
  They move when a measurement or a new feature gives a reason to, and the rule says which way
  they move. The tree filter's walk and find moved off the loop in #109. Both stream their
  answer in batches, each tagged with the generation that asked for it.
