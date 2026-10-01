# Stories

A diff shows you what changed. A Story shows you how the change *runs*. The AI in your right-hand
pane reads the change and writes a few Stories about it. Each Story is a chain of Steps that follows
the code in the order it runs, not in file order. Each Step points at a Site in the diff and makes
one claim about it. You walk a Story with the code on screen, and you can comment on any Step the
same way you would in [Review view](review.md).

Varde does not write Stories itself and never parses what the AI prints. It asks the AI to write a
file, waits for the file to appear, checks it, and walks it.

## The words

- **Story set**: everything written for one Range, under a title that says what the whole change is
  for. One file, one Range, one title.
- **Story**: one walkthrough of the change, with a name, a premise and an ordered chain of Steps.
- **Step**: one claim about one Site, with a why and what flows in and out. The cursor goes to a
  Step, and a comment is made against one.
- **Site**: a file, a side of the change (old or new), a range of lines, and what those lines held
  when the Story was written.
- **Spine**: the list of Stories in a set with each Story's Steps under it. You read it to see the
  shape of the whole before walking any of it.
- **Remainder**: the hunks of the change that no Step claims.
- **Prediction**: a three-way *why* question at a Step. Varde never grades it.
- **Range**: the two revisions a Story set is about, meaning what the head introduced over the base.

## Asking for a Story set

| Command | What it does |
|---|---|
| `:story` | Story this change. Varde picks a Range for you and asks you to confirm it. |
| `:story <range>` | Story an explicit Range, e.g. `:story main..HEAD`. |
| `:story! <range>` | Write a new Story set for a Range that already has one. |
| `:story?` | Pick a branch of this repository and story what it introduced. |
| `:story? <url>` | Clone a repository you do not have (a guest repo) and pick one of its branches. |

Palette `s` switches to Story view and shows whatever set is loaded. With nothing written yet, it
says so.

### What `:story` chooses

A bare `:story` looks at the working tree:

- **Dirty tree**: the Range is your uncommitted change against `HEAD`, written `HEAD..worktree`.
- **Clean tree**: the Range is what your branch introduced over the default branch. Varde finds the
  default branch offline, trying in order `origin/HEAD`, the upstream branch, `init.defaultBranch`,
  a branch called `main`, then `master`. It uses the first one that exists.

If none of those exist, Varde refuses with a notice rather than guessing, because a wrong Range costs
you five to eleven minutes of the AI's time on the wrong commits. It refuses an explicit Range git
cannot resolve the same way, and never shows it as an empty list.

A Range is always *what the head introduced*. Varde diffs from the merge-base, never tip against
tip. So if the base branch moved on after yours forked, your branch does not appear to delete
everything that landed on the base in the meantime.

### Confirming

Whatever the Range, Varde shows it and asks before sending anything, because sending the prompt
clears whatever the AI's command line is showing.

| Key | In the confirmation |
|---|---|
| `Enter` or `y` | send it |
| `Esc` or `n` | decline. Nothing is sent and nothing is written |

Confirming pastes the prompt into the AI pane and submits it. If no AI session is running, Varde
starts the configured one first and holds the prompt until it is ready for input (see
[AI pane](ai-pane.md)). Varde also writes a companion file,
`.varde/stories/<base>-<head>.context.md`, beside the story file and names it in the prompt, so the
AI has the change in front of it.

### A Range that already has a set

`:story` on a Range that already has a set on disk **loads it** at once, offline, without touching
the AI. Only `:story!` writes a new one, and it asks first, because that costs minutes, clears the
AI's prompt and discards your place in the old set.

## While it writes

Story view says `authoring` and waits. There is no timeout. Writing takes five to eleven minutes on
real changes, and a guessed limit would be wrong on a slow run and slow on a fast one. The wait ends
when the file arrives, when the AI exits (the view says `authoring-abandoned`), or when you press
`Esc`.

When the file arrives, Varde checks four things that need no judgement. Every Site's file exists,
its line range fits inside the file, a Site that claims a change really overlaps one, and a cited
value really appears on the line it cites. If the set fails, Varde **hands it back** to the AI with
a fix request naming only the failing Steps, and keeps waiting. It allows two rounds. If the second
file still fails, Varde refuses the set and shows what failed. If the file does not parse at all
(a missing title, a Step without a name, a Prediction without exactly three choices), Varde refuses
the whole file and never keeps part of it. A spine missing one Story would look the same as one
where the AI never wrote that Story.

## Where a Story set lives, and how long

Varde writes sets to `.varde/stories/<base>-<head>.json`, named by twelve-hex-digit prefixes of the
two revisions: `aaaaaaaaaaaa-bbbbbbbbbbbb.json` for a committed Range, `aaaaaaaaaaaa-worktree.json`
for an uncommitted one. It keeps the ten most recent with their companion files, and deletes older
sets when it writes a new one.

**A Story dies with its Range.** Varde never updates a Story to follow the code. When a Step's Site
no longer holds the text it was written against, the Step says so, shows what the Site used to
hold, and stops claiming to describe what is on screen. Varde never moves it to whatever is now in
that place. If you walk the same code next month, someone writes a new set. That cost is small, and
in return a Story written for one Range is never *wrong* about it. The reasoning is in
`docs/adr/0005-a-story-dies-with-its-range.md`.

You do not write Story sets by hand, and nothing in Varde helps you keep one around. Durable,
curated tours of a codebase would be a different feature.

## The spine

The spine is in the left-hand pane, where the file tree is in Edit view. Under the set's title it
lists each Story with its step count, plus a stale count when some of its Steps no longer match the
code. The Remainder is below the Stories.

| Key | In the spine |
|---|---|
| `Up` `Down` | move between Stories, and one row further to the Remainder |
| `Enter` | walk the selected Story, or the Remainder |
| `t` | toggle the pane between the spine and the changed-files list |
| `j` `k` | scroll |

`t` adds the changed-files list alongside the spine rather than replacing it, so you can look at the
plain diff of a file from Story view. The spine has no filter box.

### The Remainder

The Remainder is what the Stories did not reach: a count of the hunks no Step claims and a list of
the files they are in. It is never a percentage. Any denominator is a judgement about which files
deserve your attention, and a low one teaches you to ignore the line. Twelve unclaimed hunks means
twelve things to look at.

A Site claims a hunk by overlapping it, not by containing it. A Site of kind `context`, code the
Story only passes through, claims nothing. Two Stories claiming the same hunk count it once. Varde
recomputes the Remainder as the change grows, so a set written for your uncommitted work stays
accurate while the AI keeps writing code. Deletions get a line of their own, shown only when the
Range deletes something, so "this change deleted nothing" and "twelve lines went and nobody walked
them" never look the same.

Walking the Remainder steps through the unclaimed hunks as plain locations: no claim, no Prediction,
no step menu.

## Walking a Story

Enter a Story from the spine and focus moves to the editor. The cursor lands on the first Step's
Site, and the view scrolls so the whole Site fits in the pane below a little context. The Site's
lines keep their colour and get a mark, and everything else on screen is dimmed to one grey. The
mark says whether the Site is changed code or context. A band beneath the code shows the Step's
claim and, when it has them, its cited values. To the left of the gutter a step menu lists every
Step of the Story and marks the current one. The menu only shows where you are. The keys below do
the moving.

| Key | While walking |
|---|---|
| `n` `p` | next and previous Step |
| `j` `k` | scroll the code, without stepping |
| arrows | move the cursor through the code, without stepping or editing |
| `l` `h` `0` | slide the code sideways, and back to the start |
| `d` | show and hide the range's diff over the Site: added lines in green, removed lines as red rows where they were |
| `D` | open and close the Step's detail: claim, why, flow, and the nudge when there is one |
| `g` | jump to a cited value's source |
| `Ctrl+P` `Ctrl+N` | jump back and forward through cursor history |
| `c` | comment on this Step |
| `e` | open the Step's file in Edit view |
| `t` | show the changed-files list in the left-hand pane |
| `:submit` | send the review, as in Review view |
| `Esc` | close an open overlay, or leave the Story for the spine |

The code is read-only while walking, and the mode label says so. Pressing `n` on the last Step stays
on it rather than leaving the Story.

### Cited values

A Step may name concrete values to illustrate the flow. Each one records where it was copied
from, and `g` jumps there. Varde marks a value with no source as **invented**. That label is what
lets you trust the rest. Varde only checks that a cited value's characters are on the line it cites.
It cannot tell whether the value is really a literal.

### Stale Steps

A Step whose Site no longer holds the text it was written against is **stale**. The band warns you
before you read the claim, and `D` shows what the Site used to hold beside what it holds now. Three
things make a Step stale: its file is gone, its line range no longer fits, or the text changed.
Reformatting that only changes whitespace does not count. Editing the file in Edit view makes the
Step stale as soon as the text differs.

A stale Step keeps everything except the claim that it describes the screen. It still narrates,
still opens, and still takes a comment, and a stale Step is often where a comment belongs. A Step on
the *old* side of the change cannot show its text on screen and says so. Changes to the working tree
do not affect it, except when an uncommitted Range gets committed and the base moves under it.

### Where you left off

Varde keeps your place in a Story when you leave it and when you restart Varde. It discards your
place when the set is rewritten, and the view says so. Keeping a place across a rewrite would put
you on a different claim while saying it was where you left off.

## Predictions

At a Step the author chose, arriving shows a question over the code. It asks *why* the code is
written this way and not the obvious other way, or why the Story goes where it goes next. There are
exactly three reasons to choose from.

| Key | In a Prediction |
|---|---|
| `1` `2` `3` | pick a reason |
| `n` | dismiss it and step on |
| `p` | step back |
| `Esc` | close it |

A wrong pick shows why that reason is wrong and leaves the choices up, so you can pick again as
often as you like. Only a correct pick replaces the choices with its explanation, and picking again
cannot undo it. Nothing blocks you: `n` steps on with the question unanswered.

Once a Prediction has been shown, answered or skipped, Varde does not ask it again when you return to
the Step. A rewritten set asks it again. Varde records only *that* it showed a Prediction, never
which choice you picked. A history of wrong answers would be a score, and this is a reviewer reading
a colleague's change.

## Commenting from a Story

`c` on a Step opens the same comment box Review view uses, on the Step's Site. Pick `i`, `n`, `s` or
`c` for `ISSUE`, `NOTE`, `SUGGESTION` or `COMMENT`, type the body, and press `Ctrl+S` to file it.
The comment records the Story and Step it was made against, as well as the file, line range, type
and reviewed revision, so a submitted review says which *claim* you rejected. A filed comment stays
under its line for as long as that file is on screen, wherever the walk has moved.

Comments made while walking and comments made in Review view are one review. `:submit` from either
view sends them all. See [Review](review.md#submitting).

## Storying a branch

`:story?` lists the repository's branches in a picker: local and remote-tracking, with duplicates by
short name removed, most recently committed first.

| Key | In the branch picker |
|---|---|
| type | narrow the list; `Backspace` widens it again |
| `Up` `Down` | move the selection |
| `Enter` | check the branch out and story what it introduced |
| `Esc` | close |

Picking a branch **checks it out**, then sets the Range to what that branch introduced over the
default branch and asks you to confirm as usual. The checkout is necessary. Varde judges a Step
stale by what its lines hold on disk, so a set for a branch that is not checked out would report
every Step stale. Varde does not check the original branch back out afterwards, because a second
checkout can fail if files changed during the review. Story view shows which branch you are on and
which one you left.

Varde refuses the picker on a dirty working tree and tells you to commit first, because it never
checks out over unsaved work. Varde's own `.varde/` files do not count as your work. It also
refuses the picker in a folder that is not a repository.

## A Bare workspace

`varde` with no folder opens the current directory as a **Bare workspace**. The folder is the
workspace and `:w` still writes into it, but Varde writes nothing of its own there. It creates no
`.varde/` and seeds no config. Everything Varde needs for itself goes into a Sidecar under
`~/.varde/`, which Varde deletes when it exits. A Bare workspace remembers nothing between runs, by
design.

This affects reviews and Stories in two ways. Varde writes a review submitted from a Bare workspace
to `~/.varde/reviews/`, outside every workspace, so deleting the Sidecar does not delete your review.
And a Bare workspace is where you review a repository that is not on your machine.

## Guest repos

In a Bare workspace, `:story? <url>` clones the repository at that URL, a **guest repo**, into the
Sidecar and lists its branches in the same picker. The clone runs as **your own `git`**, in the
shell pane. Your SSH config, per-host keys and agent all work without being set up twice, you can
see the progress, and you can answer a passphrase or host-key prompt. Story view says `cloning`
until it finishes. If the clone fails, Varde shows its exit status rather than waiting forever. You
need `git` installed. Without it, Varde refuses the command.

Picking a guest branch checks it out *in the clone* and sets the Range against the guest repo's own
default branch. Walking opens the guest repo's files inside the clone, and Varde checks their Sites
against the clone. Those files are read-only, because Varde deletes them when it quits, so edits
would be lost. Varde refuses every key that would change one, with a message. The guest repo never
appears in the file tree, which always shows the folder you started in.

A second `:story?` on the same URL in one session **fetches** into the existing copy rather than
cloning again, so a branch pushed since the clone shows up in the picker. If the fetch fails, you
keep the copy you have. A bare `:story?` after a guest repo lists this folder's own branches again.
At exit Varde deletes the Sidecar and the clone with it, and leaves nothing behind.

Varde refuses `:story? <url>` in a project workspace (`varde <folder>`), because a guest repo needs
a Bare workspace.

## See also

- [Review](review.md): the diff, the comment box, `:submit`.
- [AI pane](ai-pane.md): the session that writes Stories, and what happens when it is not running.
- [Getting around](getting-around.md): the palette, panes and focus.
