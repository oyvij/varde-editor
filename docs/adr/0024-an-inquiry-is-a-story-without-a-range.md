# An Inquiry is a Story without a Range

A Story set may be authored for an **Inquiry** — free text the developer typed, plus one Tag — instead
of a Range. `:story*` opens a multiline box, Ctrl+S sends the Inquiry into the hosted AI session with
the schema, and the AI picks the code worth grouping into Stories and Steps. The set describes the
code as it stands on disk. There is no base, no head, and nothing claimed to have changed.

This needs writing down because `docs/adr/0005-a-story-dies-with-its-range.md` reads as a refusal of
it. 0005 is titled *A Story dies with its range*, and its Consequences send "a future contributor who
wants durable, curated tours of the codebase" away to argue for a different feature. An Inquiry is
not that contributor. It is not durable and not curated: it is one developer asking one question,
walked once and pruned like every other set. What it gives up is the *death event*, not the
disposability — and that is the whole of the trade-off here.

A Range set dies when its range merges. The range is an external fact, so the death needs no rule and
no setting: the set is for a change, and the change stops existing. An Inquiry set has no such
moment. It dies two weaker ways, both of which already exist: its Steps go stale when the lines they
were written against no longer hold that text, and the set falls out of the pool of ten as newer sets
arrive. Nothing watches for its subject to become irrelevant, because nothing can — "how does the key
reach the child" has no merge commit.

We take that as the cheap side, for the same reason 0005 took its own. The expensive artifact in this
genre is never the authoring, it is reviewing a tour that is quietly lying. An Inquiry set cannot lie
about a change it was not written against, because it was not written against a change; and when the
code under one of its Steps moves, the Step says so and says what the Site used to hold, exactly as a
Range set's Step does. The honesty mechanism is the Stale step, and the Stale step never needed a
Range.

## Considered Options

- **A synthetic Range** (`HEAD..HEAD`) so nothing downstream changes. Rejected: it makes three things
  lie at once. `claims-no-change` would fail every Step, because no Step overlaps a hunk when there
  are no hunks. Coverage would be a count over an empty set of hunks, and the Remainder would be
  empty for a range that deleted nothing and a range with nothing in it alike — indistinguishable,
  which is the exact confusion the Remainder's separate deletion count exists to prevent. Buying an
  untouched call site with three dishonest readings is not a saving.
- **A separate retention pool** for Inquiry sets. Rejected by the user: one pool of ten, as now. The
  cost is recorded under Consequences rather than hidden, because it is a real one.
- **Durable Inquiry sets** — the curated tour. Rejected: that is the feature 0005 banished, it is not
  what was asked for, and making an Inquiry set durable would reintroduce the maintenance cost that
  killed the genre.
- **Reading the AI's output instead of an artifact.** Already settled by
  `docs/adr/0006-stories-arrive-as-an-artifact.md`; an Inquiry changes nothing about that channel.

## Consequences

`protocolVersion` goes to **3**, and the bump earns its keep for a narrower reason than 0008's.
Making `range` optional does not break a version 2 artifact — those all carry one. The version is
what makes its *absence* mean something: under 2 a missing range is a malformed file, refused whole
per 0006, and under 3 it names an Inquiry set. Without the bump, a version 2 artifact that lost its
range would be silently walked as an Inquiry rather than refused, which is salvaging a broken file in
part.

Coverage and the Remainder are not shown for an Inquiry set. This is not a degraded view of them:
there are no hunks to divide, and a Coverage over no hunks is the ratio its own glossary entry
refuses by name. Of the four checks, `claims-no-change` cannot run and the other three —
`file-missing`, `range-out-of-bounds`, `citation-absent` — do, so the two-round fix request and the
refusal after the second are unchanged.

Everything a reviewer actually walks needs no Range and is untouched: the Spine, the Step-menu, the
Site mark, Predictions, Nudges, Cited values and Stale steps. A comment made on an Inquiry Step
records `worktree` as its revision — already a legal head naming the uncommitted change, and
literally true, since the comment was made against the files as they stand.

An Inquiry needs no git repository. Nothing it does asks git a question: the Site text is read off
disk, staleness is a disk read, and the artifact is named from a timestamp
(`.varde/stories/inquiry-<unix-ms>.json`) rather than from two commit oids, so it works in a Bare
workspace with the set in the Sidecar.

Sharing one pool of ten means an Inquiry set can evict the Range set for a review in flight, and the
Range set is the cheaper of the two to get back — Varde resolves a Range for you, while an Inquiry is
only re-askable by whoever remembers what they typed. Ten was chosen in 0005 for size, against sets
nobody meant to read twice; if that eviction ever bites in practice, a second pool is the fix, not a
larger one.
