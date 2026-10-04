# A Story dies with its range

A Story is authored for one revision range, named for it on disk
(`.varde/stories/<base12>-<head12>.json`), and expected to be gone once that range merges. Varde
never updates a Story to follow the code. When a Step's Site no longer holds the text it was written
against, the Step says so, says what the Site used to hold, and stops claiming to describe what is on
screen — it is never quietly re-pointed at whatever moved into its place.

Every tool in the surveyed genre made the opposite choice, and the genre is a graveyard. Twenty-one
sources were read; the pattern is the same in all of them. CodeTour has 464,000 installs and
"automatically updating a tour file as the associated code changes" has sat unshipped in its
`## Upcoming` section since before its last release. Its steps degrade *silently*: a stale step shows
a wrong line with exactly the confidence of a right one, and a step anchored by a pattern that no
longer matches lands at end-of-file. Taylor & Clarke declined to study whether developers would
author tours at all. The single documented reason this whole category dies is **maintenance cost** —
a durable tour is a second copy of the code, hand-maintained, and nobody maintains it.

The trade-off we took is that a Story cannot be reused. Walk the same code next month and somebody
pays to author it again — five to eleven minutes of a hosted CLI's time, measured across three real
runs. We think that is the cheap side. A Story generated per range is never *wrong*, because it never
has to survive a change it was not written against; the expensive artifact in the alternative is not
the authoring, it is the reviewing of a tour that is quietly lying. And the authoring is no longer a
person's afternoon: it is a prompt and a file watcher.

Two things follow that would otherwise look like omissions. Retention is **ten** story sets, not the
fifty Varde keeps of reviews, because a story set is 30–70KB against a review's 1KB and none of them
are meant to be read again. And a Walkthrough — one person's position in a Story — is **discarded
outright** when the Story is re-authored rather than being carried across by story name and step
index. Carrying it across would put the reviewer on a different claim while telling them it is where
they left off, which is CodeTour's silent degradation reintroduced in the one place we promised not
to reproduce it.

## Consequences

Nobody can hand-write a Story and expect it to last, and nothing in Varde will help them try. A
future contributor who wants durable, curated tours of the codebase — onboarding material, an
architecture walk — is not asking for a longer-lived Story; they are asking for a different feature,
and it should be argued for on its own terms rather than added as a retention setting here. One
such feature has since been argued and taken:
`docs/adr/0024-an-inquiry-is-a-story-without-a-range.md` authors a Story set from free text about
the code as it stands. It is not the durable tour this paragraph turns away — it is as
disposable as a Story set for a Range, and what it gives up is the clean death this ADR is named
for.

Fuzzy re-anchoring — following a moved Step by searching for its text nearby — is not forbidden by
this decision, only unnecessary for it. It would make a Story survive small movements *within* its
range, which is a different thing from surviving the range. `git2` cannot supply it: its blame
copy-tracking options are documented no-ops and it offers no line-to-line mapping across revisions at
all, so it would need a crate of its own.
