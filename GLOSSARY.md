# Varde

An IDE TUI: it opens on a folder and presents it as a workspace — a file tree, buffers, a diff
review, a shell and an AI session, side by side in one terminal.

This file is the glossary and nothing else. `AGENTS.md` holds the working contract;
`docs/example-map.md` holds the spec; `docs/adr/` holds the decisions.

## Language

### What it is called

**Varde**:
The editor. A stone cairn somebody stacked to mark the way for whoever comes next. Written `Varde`
in prose, `varde` as the binary, the crate, the state directories and the release assets, and
`VARDE` in nothing but the XTVERSION reply, where uppercase is the sequence's convention.
_Avoid_: CRIME, the TUI, the tool, Varde Editor

**CRIME**:
What Varde was called until 0.161.1, and an acronym — Command · Review · Integrated · Modal ·
Editor. Retired: `docs/adr/0020-the-editor-is-called-varde.md`. It survives only in that ADR and in
release history, and a CRIME install does not update into a Varde one.
_Avoid_: using it at all

### Selecting and copying

**Selection**:
The text the workspace currently holds as "what you picked". There is exactly one, and it is what
copying copies — a mouse drag and a keyboard extend produce the same thing.
_Avoid_: highlight, marked text, visual selection

**Charwise**:
A selection measured in characters, which may start and end mid-line.
_Avoid_: character mode, inline selection

**Linewise**:
A selection measured in whole lines, ends included. What a review comment covers.
_Avoid_: line mode, block selection

**Anchor**:
The end of a selection that stays put while the other end moves.
_Avoid_: start, origin, mark

**Extending**:
Growing or shrinking a selection by moving the cursor while the anchor holds.
_Avoid_: expanding, dragging out

**Row selection**:
The highlighted row in a list pane — the file tree, or whichever list is in the Corner. It names
something to go to rather than text you picked, so it is never copied as characters and never
becomes the selection.
_Avoid_: selected file, tree highlight

**Corner**:
The one pane-sized slot beneath the file tree, at the tree's width, taking its columns from the
shell. It names its occupant — the Risk list, the Buffers pane, the Cursor history, the Frames, the
Breakpoint list, the Diagnostic list or the Conflict list — or nothing at all, so "both on screen at once" is not a state it can hold and
asking for one while another shows is a replacement. Every occupant is the same rectangle: which
pane is in the Corner changes what a click means, never where the Corner is.
_Avoid_: the risk pane's slot, bottom-left pane, second sidebar

**Strip**:
The slot along the bottom of the screen, its height the user's to drag. Like the Corner it names one
occupant at a time — the Shell group, or the Debug group while a Debug session exists — so showing
one hides the other without stopping anything running in it.
_Avoid_: terminal pane, bottom panel, dock, tool window

**Shell group**:
Every shell split in the Strip, shown and hidden as one.
_Avoid_: terminals, terminal tab

**Group tab**:
One of the names on the Strip's top border — `Shells`, and `Debug` while a Debug session exists —
lit for the group showing, and clicked to show another.
_Avoid_: tab (a Buffer is not one either), switcher

**Copying**:
Putting the selection on the system clipboard, falling back to the terminal over SSH.
_Avoid_: cut, clip

**Yanking**:
Putting text in the register. Yanking also copies; the register is for putting text back inside
the workspace, the clipboard for carrying it out.
_Avoid_: copy (in the vim sense)

**Register**:
The single slot yanking fills and putting reads.
_Avoid_: clipboard, buffer, kill ring

### Moving and finding

**Motion**:
A keypress that moves the cursor without changing the text. Every motion is reachable without a
modifier; modifier bindings are aliases for one.
_Avoid_: navigation, movement command

**In-file search**:
Looking inside the buffer you are editing and moving the cursor to a match. Once started it stays
on — its line on the editor's border, its highlights, `n` and `N` — until Escape, whichever pane has
the keyboard; the query line holds the keyboard only while you type in it.
_Avoid_: local search, find

**Replace box**:
The small floating box the in-file search's replace icons open: the query, what to put in its place,
and whether to replace the one Match at the cursor or every Match in the buffer. It edits the buffer
and never the disk, and it shares the search's case toggle rather than holding one of its own.
_Avoid_: replace dialog, substitute, find-and-replace

**Match**:
A place in the current buffer where the in-file search query occurs — somewhere you go.
_Avoid_: hit, result

**Project search**:
Looking across every file in the workspace and listing hits grouped by file.
_Avoid_: global search, grep

**Hit**:
An occurrence the project search found, in a file you may not have open — a place to open.
_Avoid_: match, result

**Result row**:
One row of the project search's box: a file heading, or a hit beneath one. A hit's row is its index
*plus the headings above it*, which is why the renderer and the scroll clamp read the same rows
rather than counting hits.
_Avoid_: line, entry

**Visit**:
One place the cursor has been: a file, a line, a column, and the text that line held at the time.
Always a **source** line, even when it was taken from a Preview, whose cursor is a rendered row — a
row stored there names a line nobody was on and excerpts whatever line of that number happens to
hold. Recorded only by a Jump, and recorded as the place being *left*, so going back returns you
where you were. The text is carried rather than looked up, so a row still says something about a
file that has since been edited — and can be marked Stale rather than claimed to be current.
_Avoid_: mark, position, jump point, breadcrumb, history entry

**Jump**:
A long-distance move of the cursor, made deliberately: opening a file, switching Buffer, an in-file
search landing, the ends of a file. A Jump records a Visit; a Motion does not, and neither does a
click, and neither does browsing the tree — a preview the next preview replaces is a file you passed,
not a file you went to. It is what "back" and "forward" step between.
_Avoid_: navigation, goto, move

**Landing**:
Putting the cursor on a place something else chose: a Visit, a search Hit, a Risk row's line, a
definition, the line a crossing to Preview came from. The place is always a source line, so the
landing is where the crossing into a Preview's rendered rows is made — one for all of them, because
a landing that forgot it left the caret on the top of the render while an invisible second cursor
sat on the line nobody could see.
_Avoid_: goto, seek, reveal

**Cursor history**:
The Visits, oldest first, and a cursor of its own into them — the row the pane highlights and the
position back and forward move. A log of where you have been rather than a tree: a Jump made while
travelling appends rather than truncating what was ahead. This session's, never written down, and
capped, so it stays a list you can read. Its cursor sits *past the newest* Visit whenever you are
somewhere newer than everything recorded, which is no row at all.
_Avoid_: jumplist, back stack, trail, breadcrumbs

**Helper row**:
The dim row inside a box naming the keys that box answers — the results box has one, drawn from the
list that lives beside the router. Inside the box, not in its border: it names the way through what
the box is showing.
_Avoid_: footer, hint, legend

### Showing what is open

**Buffer**:
A file the workspace is holding open, saved or not. Opening never closes the previous one.
_Avoid_: tab, document, editor

**Preview**:
A markdown Buffer shown as the document it describes rather than as the characters it holds:
headings, prose, lists, tables and diagrams, laid out to the pane. Read-only — a Preview is a
rendering, and the file it renders is changed as Source, which is where `i` and `:format` both cross
to before a character moves. Not a View: Edit, Review and Story are where
you are in the workspace, and a Preview is one Buffer's way of being drawn inside Edit.
_Avoid_: formatted view, rendered view, markdown mode, reading mode

**Rendered column**:
Which character of a Preview row the cursor is on, counted in characters of what is *drawn* — never
a column of the source line behind it, which the row map cannot carry. It is the coordinate a
Preview's motions, its caret, its footer position and the sideways offset all speak in, and it
starts over at one whenever the cursor crosses between Preview and Source.
_Avoid_: screen column, display column, x

**Source**:
A Buffer shown as the characters the file holds, markup included. The only way a file is edited: the
switch to Source is the switch to being able to change it.
_Avoid_: raw (that is unsanitised bytes elsewhere in this repo), plain, unformatted

**Authorship**:
Who last committed the line the cursor is on, and the day they wrote it, said on the editor's top
border. It is the *committed* file's answer, so a line the working tree has changed — and every line
of a file the commit has no copy of — is **not committed yet** rather than somebody's. Outside a
repository, and without git, there is no Authorship at all and the border says nothing: the
Change bar's answer to the same question — and the same diff, since the bar marks exactly the lines
that have no Authorship yet.
_Avoid_: blame (that is git's command, and what it names is the whole file), attribution, ownership,
last-modified

**Buffer mark**:
The glyph on a file tree row saying whether that file is open, current, or unsaved.
_Avoid_: badge, indicator, icon

**Change bar**:
The bar in the gutter beside a line the last commit does not hold — inserted or edited, saved or
not — so every place a file was changed is visible while editing it. It answers the buffer, not the
disk, and a file the commit has no copy of carries none. The quietest of the three marks the gutter's
one column can hold: a diagnostic and the voice's place both take it first.
_Avoid_: gutter indicator, diff decoration, modified marker, git gutter

**Conflict**:
A region of a file's text that git's merge left undecided — a current side and an incoming side,
and sometimes their common ancestor, between marker lines. Drawn as an overlay with its sides
labelled, but only ever text: accepting a side is an edit to the buffer like any other, and nothing
Varde does stages it.
_Avoid_: merge conflict marker, hunk, clash

**Conflict list**:
The Corner occupant listing every file git reports as unmerged and, under each, the Conflicts still
in its text. A file whose Conflicts are all gone stays listed until git stops reporting it.
_Avoid_: merge editor, conflicts pane, unmerged files

**Diagnostic**:
One Language server's report of something at a range in a file, with a Severity: Error, Warning,
Information or Hint. Kept per file and per server, since a push replaces only its own server's last
report, and held for files nobody has open as much as for Buffers.
_Avoid_: error, problem, lint, issue (each names one Severity or one source, not the thing)

**Diagnostic list**:
The Corner occupant listing every Diagnostic in the project, one Severity at a time, grouped under a
heading per file. It opens on the most severe Severity that has any, because that is what needs
fixing first.
_Avoid_: problems panel, error list, diagnostics pane

**Severity label**:
One of the four names on the Diagnostic list's top border — Errors, Warnings, Info, Hints — each with
its count, lit for the Severity showing and clicked or lettered to show another.
_Avoid_: tab, filter

**Cheatsheet**:
The read-only reminder of the keys that work in the current view — motions, chords, the gestures
pressed in a pane. It takes the AI pane's place when asked for, with the AI session still running
behind it, and it scrolls and takes focus like any pane. It lists keys and never Commands.
_Avoid_: help, legend, hints, key list

**Command**:
A word typed after `:` that does one thing wherever it is typed — `minimap`, `ai`, `story`. Not a
key: a key means something in the pane it is pressed in, and a Command does not.
_Avoid_: ex command, action, key

**Command list**:
The list under the `:` line of every Command that belongs to the current view — whether or
not it would do anything right now — narrowed with each character typed. The arrow keys
and Enter choose one, which fills the line; it runs only on the Enter after that, so the list finds
a Command and never runs one. It lists Commands and never keys.
_Avoid_: dropdown, completion, command palette (the palette is a different thing)

**Minimap**:
A far-off mirror of the whole file down the editor's right-hand edge, two lines to a row and four
source columns to a cell — the shape of the file rather than its text. Press or drag in it to
travel. Not a pane: it takes columns out of the editor's own rectangle and is hit-tested inside it,
the way the gutter is. Its last column is the **mark lane**: a row holding an Error, a Warning or a
changed line is marked in the gutter's colour for it, the most important of its two lines winning,
so a problem off screen can be seen without scrolling to it. Information, Hint and the reading
place never mark it.
_Avoid_: overview, preview (that is a markdown Buffer here), thumbnail, bird's-eye view

**Slider**:
The line down the Minimap's first column, beside the rows the editor's own window covers — where
you are, in the mirror. A line and not a field: a mirror this faint is mostly blank, so anything
painted behind it is the loudest thing on the pane. **Lit** while the pointer is on the mirror,
which is also the whole of the travelling gesture, and quiet otherwise.
_Avoid_: viewport box, highlight, region

**Thumb**:
The line in the editor's last column saying how far through the file the window is — drawn only
where there is no Minimap to read it off, and absent altogether when the whole file is on screen.
One indicator at a time: beside a mirror, the Slider already says it.
_Avoid_: scrollbar (that is the thumb and its track, and there is no track), handle, grip

### Hosting a child process

**Hosted pane**:
A pane whose keyboard, mouse and clipboard belong to the child process running in it. The AI pane
and the shell pane.
_Avoid_: terminal pane, passthrough pane, pty pane

**Reserved key**:
A key Varde claims from a hosted pane. An exhaustive list, not a policy: a key is reserved because
it is on the list, and everything not on it reaches the child.
_Avoid_: global key, binding, shortcut

**Palette tap**:
A key whose double tap opens the palette. Exactly one is armed per terminal, whichever gesture that
terminal can report. Where the armed one is a key a child could otherwise have received, it is
passed on as well as counted, so it is not a Reserved key.
_Avoid_: escape hatch, fallback binding, chord

### Staying up to date

**Version**:
What a checkout claims about itself. It is a claim, not an observation: it moves when somebody
moves it, and a checkout whose code has changed without it is telling you something untrue.
_Avoid_: release, tag, build number, revision

**Running version**:
What the binary you are talking to was compiled from — the one thing a running Varde knows for
certain about itself, because it was baked in when it was built.
_Avoid_: current version, installed version, binary version

**Update**:
A Version strictly newer than the Running version: the checkout — or, for a binary install, the
latest Release — has moved on and the binary has not. A Version that is equal or older is not an
Update, so there is nothing to offer.
_Avoid_: upgrade, new release, available version, newer build

**Install kind**:
How this Varde got onto the machine: a **checkout install** (the binary sits inside its own
checkout, which is `target/release/varde` under a manifest naming varde) or a **binary install**
(anything else). Decided once at startup from where the binary is. Only a binary install asks the
network anything.
_Avoid_: install mode, distribution, channel

**Release**:
A published Version with one binary per platform and a checksum list, a GitHub Release of this
repository. A Release is what a binary install compares itself against; a checkout install never
looks for one.
_Avoid_: tag, build, download

**Asset**:
The one file in a Release built for this platform, named `varde-<os>-<arch>`. A Release with no
Asset for this platform offers no Update.
_Avoid_: artifact, package, binary (that is what is running)

**Relaunch**:
Varde replacing itself with the binary now on disk, keeping its arguments. Not a restart of the
session: the shell and the AI pane end with the old process, as they do on quit.
_Avoid_: restart, reload, reboot

### Settling the project's answers

**Indent width**:
How many spaces one level of indentation is, as the project says: `editor.tab_width` in the layered
config, four when no layer names one. It is what Tab lays down, and what opening a block *falls back
to* — a file's own shallowest indentation beats it, because the lines already there are better
evidence about that file than a configured number is.
_Avoid_: tab size, tab stop (that is where a Candidate left a blank), shift width, indentation

**Setting**:
A value Varde cannot work without — an indent width, a double-tap window, a threshold, a speed. It has
a built-in answer that a config file may beat, and a file that does not name it changes nothing.
_Avoid_: option, preference, program (that is a row naming something Varde starts)

**Program row**:
A config table naming something Varde starts — a language server, a formatter, the voice — with the
files it serves and the command that installs it. Program rows exist only in a config file: a row no
file names does not run, and the ones Varde knows about but the user has not taken are *available*,
not configured, until taken from Tools.
_Avoid_: default, built-in server, plugin, integration

**Tools**:
The one list of everything Varde runs — language servers, formatters, requirements and speech —
each row with its status and one key that takes it: configured in the global file and installed.
It shows the rows the reader has and the ones Varde knows about that they have not taken yet.
_Avoid_: servers list, formatter list, plugins, extensions, marketplace

**Seeding**:
Writing a file the first time Varde opens a folder, and only when nothing is there — the project's
`config.toml`, whose every key arrives commented out. The global `config.toml` is seeded the same
way when it is missing, except that its Program rows arrive live, since nothing else will run them. A key nobody can find is a key nobody sets,
which is the whole reason it is written at all; a live value in it would be this binary's answer
frozen into a file that outlives it — and so would a commented one gone stale, so a test uncomments
them and holds each against the shipped defaults. "Nothing is there" is the config layer the edge
already read being absent, so the decision is the core's and the write is an ordinary one.
_Avoid_: scaffolding, generating, initialising, installing, creating (that is the folder)

**Bare workspace**:
A workspace Varde writes nothing into — opened by naming no folder at all, which is the whole of how
it is asked for. The folder is still the workspace: the file tree is it, and `:w` writes there.
Everything that would have gone in the project's `.varde/` goes in a Sidecar instead, so a Bare
workspace forgets everything between runs and seeds nothing, measures no Risk unasked, and cannot be
told to remember — one that remembers is a project
(`docs/adr/0016-a-bare-workspace-leaves-nothing-behind.md`).
_Avoid_: bare-bone instance, temp instance, scratch workspace, editor mode

**Single-file editor**:
Varde opened on one file rather than a folder: the editor alone, full screen, with every editor
capability and no other pane. It writes nothing but the file itself, remembers nothing between runs,
and gives a Language server the file's own folder. Not a workspace — there is no tree to be one.
_Avoid_: file mode, lite mode, bare file, scratch editor

**Sidecar**:
Where a Bare workspace's own state lives — under `~/.varde/paths/`, named for the folder and the
process, deleted when Varde exits and swept on start when a crash escaped that. It holds what belongs
to Varde, never what belongs to the user: a Guest repo, a story set, a session's `state.json`. A
submitted review is the one thing that does not go here, because an output destroyed at exit is a
different kind of nothing than a trace not left.
_Avoid_: scratch (taken twice — `.scratch/` was the old issue tracker, `~/.varde/tmp/` is a Reading's
audio), shadow, cache, temp folder

### Walking a change

**Range**:
The two revisions a Story set is authored for, and the spelling that names them: a base, a head, and
the text a reviewer typed or Varde resolved. Always the change the head *introduced* — a merge-base
against the base, never tip against tip, because a base that has moved on since the head forked
would otherwise read as the head deleting everything that landed meanwhile. `worktree` is a legal
head and names the uncommitted change; there is only one working tree, so there is only one of those.
_Avoid_: diff, revision range (say Range), commit range, base..head

**Inquiry**:
What a developer typed to ask for a Story set: free text and one Tag, naming what they want
explained. The alternative to a Range — a Story set is authored for one or the other. An Inquiry
describes the code as it stands on disk, so it has no base, no head and nothing changed, and it dies
by going stale and by being pruned rather than by its Range merging
(`docs/adr/0024-an-inquiry-is-a-story-without-a-range.md`).
_Avoid_: prompt (that is what Varde sends the AI), query, ask, question, freetext

**Tag**:
The one word a developer picks beside an Inquiry to say what kind of thing it is about — a bug, a
feature, the architecture, or nothing in particular. One at a time, and it scopes what the AI is
asked for; it is never recorded in the Story set.
_Avoid_: label, category, kind, topic, scope

**Guest repo**:
A repository Varde cloned into a Sidecar to author a Story set for a branch of it — somebody else's
code, on somebody else's remote, present for one session. Its files are never in the file tree and
are never saved to: a Guest repo is read, and it is gone at exit. Cloned and fetched by the user's own
`git`, never by `git2` (`docs/adr/0015-a-clone-is-the-users-own-git.md`).
_Avoid_: temp clone, external repo, checkout, scratch repo

**Repository under review**:
Whichever repository a Story set's git questions are asked of — the Guest repo when one has been
cloned, the workspace otherwise (`State::repo_root`). Not the same thing as the workspace: the tree,
a save and what `.gitignore` covers are the folder Varde was opened on, while a range, a checkout and
a Step's staleness belong to the repository the Story describes.
_Avoid_: workspace root (that is the folder), target repo, current repo

**Story set**:
Every Story authored for one Range or one Inquiry, plus a title naming what it accomplishes at large
— a feature added, a bug fixed, an improvement made — so a reviewer with zero context knows what the
whole set is for before reading a single Story. One file, one title, and one Range or one Inquiry
(`docs/adr/0005-a-story-dies-with-its-range.md`).
_Avoid_: story artifact, story batch, change (too generic — this is specifically the authored set)

**Story**:
One narrative through a change: a name, a premise and an ordered chain of steps that follows how the
code runs rather than how the files are arranged. Authored, durable for the life of the review, and
readable by someone who did not write it. A Story set may hold several. A Story's name states the
concrete subject — what changed — never a standalone metaphor; the premise is where the *why* goes.
_Avoid_: tour, trail, walkthrough (that is the act), narrative, guide

**Step**:
The atom of a Story — a name, one claim about one Site, with why it exists and what flows in and
out of it. A Step is where the cursor goes and what a comment can be made against. Its name is
short enough to sit in the step-menu; its claim is the full sentence the band narrates.
_Avoid_: stop, node, frame, slide

**Step-menu**:
The list of a Story's Steps, named, shown to the left of the gutter while walking one, with the
current Step marked. Read-only — it shows where you are; `n`/`p` still do the moving. Never shown
while walking the Remainder, which has no names to show.
_Avoid_: step list, step sidebar, outline

**Site**:
Where in the code a Step points: a file, a side of the change, a range of lines, and the text those
lines held when the Story was written. Not an Anchor — that word is already the fixed end of a
selection, and one word for two things is how a glossary stops being one.
_Avoid_: anchor, location, position, target, region

**Walkthrough**:
One person's position in a Story: what has been walked, what was skipped, where they are. Started,
stopped and restarted freely; disposable, and never shared. A Story is authored, a Walkthrough is
lived.
_Avoid_: session, progress, run, playthrough

**Site mark**:
How the code on screen says which lines the current Step points at: the Site's lines carry a bar and
their colour, everything else is flattened to one grey. It is drawn from where the Walkthrough
stands, never asked for and never dismissed — a reviewer sent to a file with no idea which lines the
claim is about is reading a screen of code, not a Step. Not a Selection: it is not copied, not
extended, and it exists whether or not anything is selected.
_Avoid_: highlight, selection, focus, cursor range

**Spine**:
The ordered list of a Story's steps, and of the Stories a Story set holds, under its title. What
the reviewer reads to see the shape of the whole before walking any of it.
_Avoid_: outline, table of contents, index, timeline

**Prediction**:
A question at a Step about *why* - why the code is written this way and not the obvious other way,
why the Story goes where it goes next. Three reasons to choose between, one keypress, never graded,
never blocking, and a wrong pick is told why it is wrong and may pick again. It exists to make the
reviewer commit to a reason, which is what turns reading into comprehension. Never a bet on which
line runs next: that is trivia, and it measures nothing.
_Avoid_: quiz, question, test, checkpoint, score

**Nudge**:
The extra sentence a Step holds in reserve for a reviewer who wants more, written when the Story was
written rather than fetched on demand.
_Avoid_: hint, tooltip, expansion, detail

**Cited value**:
A concrete value a Step uses to make the flow real, carrying the place it was copied from so the
reviewer can go and check it. A value with nowhere to point is invented, and is shown as invented —
the distinction is the whole reason a reviewer can trust the rest.
_Avoid_: example, sample, mock, simulated value

**Stale step**:
A Step whose Site no longer holds the text it was written against. It says so, says what the Site
used to hold, and stops claiming to describe what is on screen; it is never quietly re-pointed at
whatever moved into its place.
_Avoid_: broken, outdated, drifted, invalid

**Coverage**:
Which of a change's hunks the Stories claim. A Range has it and an Inquiry does not: there are no
hunks to divide. Never a ratio: a percentage needs a denominator, every denominator is a judgement
about which files deserve a reviewer's eye, and a low percentage teaches the reviewer to ignore the
line. Coverage is a count and a list. The word is this and only this: the tested-lines ratio a CRAP
figure needs is *test coverage*, always spelled out, because it is the ratio this entry exists to
refuse.
_Avoid_: completeness, progress, percentage, score

**Remainder**:
The hunks no Step claims, once the Stories are subtracted from what git reports. Like Coverage, it
belongs to a Range and not to an Inquiry. It is walkable but it is not a Story - no premise, no
claim, nothing authored - and it says nothing about whether leaving those hunks out was wrong.
Deletions are counted separately, so a range that deleted nothing is distinguishable from a range
whose deletions nobody walked.
_Avoid_: gap, leftovers, uncovered, missed

### Paying down risk

**Risk**:
What the workspace says about how hard its own code will be to change safely — measured per Function
and counted across the Scope, computed rather than authored. Not a claim about correctness: code can
be risky and right, and a Risk figure never says a Function is wrong.
_Avoid_: quality, debt, smell, health, score (bare — say which metric)

**CX**:
The Risk metric that needs nothing installed and nothing configured: complexity alone, with no test
coverage in it. What a workspace shows unless it can do better.
_Avoid_: complexity score, CRAP (that name claims more than CX measured)

**CRAP**:
What CX becomes once test coverage is in play — complexity weighted by how much of the Function is
tested. The name is earned by having read coverage, never assumed: a figure computed without it is a
CX figure wearing a better name, and the label is how you tell which one you are looking at.
_Avoid_: crap score, risk score, quality score

**Function**:
The unit Risk is measured against and the unit a refactor moves: a function or method whose
enclosing space is not itself a function. A closure counts toward the Function holding it and is
never listed alone — complexity that can be relocated into a nested space is complexity nobody has
to pay down. Containers are read and never listed, because a container's figure is the figures
inside it counted twice.
_Avoid_: method, symbol, unit, space (the analyser's word for any region, containers included)

**Risk count**:
How many Functions in the Scope sit above the threshold. A count and a list, for the reason Coverage
is one, and the figure the Refactor loop exists to move.
_Avoid_: total, percentage, average, grade

**Scope**:
What a Risk figure or a Refactor loop covers: the whole workspace, or the files under Review. Never
a mix — one figure describing two different sets of files is a figure nobody can act on.
_Avoid_: target, selection (that is text you picked), range (that is a Story set's)

**Refactor loop**:
Varde's own iteration over an AI session: it hands the session a Scope and a goal, waits to be told
a pass is finished, then measures the workspace and decides whether that pass stands. The deciding
is Varde's. A session that reports success is reporting what it believes, and belief is not a
measurement.
_Avoid_: agent loop, auto-refactor, AI run, automation

**Iteration**:
One pass of the Refactor loop — one prompt, one set of edits, one measurement, one verdict. An
Iteration whose edits fail the Gate leaves nothing behind in the workspace, so the workspace after a
loop is a workspace where every Iteration passed.
_Avoid_: round, attempt, turn, step (that is a Story's atom)

**Gate**:
What an Iteration's edits must satisfy to stand: the tests still pass, the Risk count or total moved
down, and no other metric moved up. Failing any one of them returns the workspace to what it held
when the Iteration began. The Gate is why "reduce this number" cannot be satisfied by scattering
complexity somewhere the number does not look — which the prompt now says outright as well, so a
session hears it before spending an Iteration on a pass the Gate would put back.
_Avoid_: check, validation, acceptance criteria, guardrail

**Stale figure**:
A Risk figure whose workspace has moved since it was computed. It says so rather than being drawn as
current, and it is never quietly recomputed while you type — a figure that churns per keystroke is
noise, and one that lies about being current is worse.
_Avoid_: outdated, dirty, invalid, pending

**Unparsed**:
A file in a language the analyser handles that it could not read. Counted and shown, never skipped
in silence: a Risk figure that quietly omits what it failed to read is claiming a Scope it never
covered.
_Avoid_: skipped, failed, ignored (that is what .gitignore does), unsupported (that is a language
nobody promised)

### Knowing what the code means

**Language server**:
A child process Varde hosts to answer questions about code it has no other way to answer — what a
name is, where it is defined, what is wrong with it. Named in configuration and never in a branch, for
the reason a CLI in a Hosted pane is never named (`docs/adr/0011-a-language-server-is-a-second-hosted-child.md`).
It has no pane, so it is not a Hosted pane; it is the other kind of hosted child.
_Avoid_: LSP (that is the protocol, not the process), backend, provider, engine, analyser (that is
what computes Risk)

**Document version**:
Which state of a Buffer a message is about. It is the Buffer's revision — bumped by every content
change and nothing else — sent with the text and quoted back in what the server says about it. A
message naming any other version describes text the user has already edited past and is dropped —
and so is a reply to something Varde asked, measured against the version it was asked at, which is
the same rule read the other way round.
_Avoid_: sequence, generation, timestamp, revision number (say revision, or Document version)

**Diagnostic**:
Something a Language server says is wrong with a range of a document: a Severity, a message, and the
lines it covers. Pushed rather than asked for, held per file so it survives switching buffers, and
replaced wholesale when the server pushes again — never appended to. It describes the present or it
is discarded; a Diagnostic that outlived its conversation is not a record, it is a lie about the
screen.
_Avoid_: error, warning (those are Severities, not the thing), problem, marker, lint, squiggle

**Severity**:
How much a Diagnostic matters: error, warning, information or hint. Four values, distinguished on
screen, because a hint drawn like an error is a gutter nobody reads.
_Avoid_: level, priority, kind (that is a highlighting token's), type

**Hover**:
What a Language server says the symbol under the cursor *is* — its type, and its documentation if the
server has any. Asked for, shown where it does not cover the symbol it describes, and dismissed. A
symbol the server knows nothing about is told so; an empty box is not an answer. Almost every server
answers in markdown, so a Hover holds the same Rows a Preview does rather than the characters the
server sent — a reply the server labelled plain text is the one that is not read as markdown.
_Avoid_: tooltip, popup (that is the Candidate list's shape), info, docs, quick info

**Definition**:
Where a name is introduced — a file and a line, somewhere to go. A server may answer with several,
and several are places to open, which is what a Hit already is: they are shown in the list the
project search fills, rather than the first being taken quietly.
_Avoid_: declaration, source, target, reference (find-references is a different question, not in scope)

**Candidate**:
One of the things a Language server offers as what you might be typing. Chosen from a list with the
keys the rest of the workspace uses, and inserted as the text it holds — never reformatted, and
expanded only where the server marked it a snippet and Varde said it could receive one. Dismissing
leaves the Buffer holding exactly the characters that were typed, which is the promise the whole
feature stands on.
_Avoid_: completion (that is the act), suggestion, item, proposal

**Tab stop**:
A place an expanded Candidate left for a value to go, and the sequence of them Tab walks. The
protocol's own word, and the one thing here that outlives the keystroke that made it: while stops
are left, Tab means "the next one" and means nothing otherwise. Escape leaves the stops and keeps
the text, because abandoning a Candidate is not undoing it — and leaving the file or the pane ends
the sequence rather than carrying places into a file they do not name.
_Avoid_: placeholder (that is the `${1:…}` in the reply, not the place in the Buffer), field, blank
(the cheatsheet's word for the gesture, not the thing), marker, anchor

**Trigger character**:
A character a Language server named as one it wants to be told about, so that it can lay out the text
around it the moment it is typed. The server's own list, read off its `initialize` reply and never
spelled in Varde: rust-analyzer names `.`, `=`, `<`, `>`, `{`, `(`, `|` and `+`, jdtls names `;`, `}`
and a newline, clangd names a newline alone, and gopls and the TypeScript server name none. What
comes back is edits, applied as one thing to undo — and dropped when the reader has typed on since,
which is the Document version rule read the other way round.
_Avoid_: format key, hotkey, on-type character, brace (that is one of them, not the idea), autoformat

**Formatter**:
An external command a project names to lay one language's files out — `prettier`, `black`, `gofmt`.
Named in a `[formatter.<language>]` row and never in a branch, for the reason a Language server is
never named. It is asked only where no server will answer, it is handed the Buffer on stdin and
answers on stdout, and it never touches the file: `:w` is the reader's. One process per `:format`,
so nothing about it is remembered — which is why a Formatter installed a moment ago works a moment
later.
_Avoid_: linter (that reports, this rewrites), prettifier, beautifier, pretty-printer, formatting
provider (that is the server's capability, not this)

**Not measured**:
What a file reads as when nothing has answered for it yet — no server for its language, or a server
that has not spoken. Distinct from zero, which is an answer. Telling a reviewer that a file with no
server is clean is the failure this word exists to refuse; it is the same distinction Unparsed draws
for Risk.
_Avoid_: none, zero, empty, n/a, pending

### Reading aloud

**Reading**:
The act of speaking a document's prose, and the thing that can be in flight, paused or stopped. A
Reading covers the Selection and nothing else — there is no reading from the cursor and no reading
of a whole file, because the passage worth hearing is the one you picked. Starting one while another
is in flight replaces it rather than queueing behind it, the same way a Risk generation supersedes
the answer still coming.
_Avoid_: playback, narration, TTS, speech, audio, play

**Utterance**:
One sentence-sized run of speech within a Reading — what previous and next move by, and what the
marker on screen names. Sentences are the unit because a Reading is built as one continuous stream
with real silence between them: a separate player per sentence leaves a gap the machine chose rather
than one the listener needs, which is audible as staccato and is the reason this word does not mean
"a clip".
_Avoid_: chunk, clip, segment, phrase, sentence (the text is a sentence; this is the speech of one)

**Transport**:
A row of Chips on a pane's top border, driving something in flight: a Reading on the editor's —
play and pause, previous, next, stop, and the speed, drawn only for a markdown buffer — and a Debug
session on the Variables' — continue and pause, stepping over, into and out, and stop. Right-aligned
the way the Risk pane's action icons are, and clickable. It is an affordance and a reminder, never
the only way in: everything it offers has a key binding, because a control you can only reach with
a mouse is one the cheatsheet cannot promise.
_Avoid_: play button, toolbar, controls, player bar, media bar, debug toolbar

**Chip**:
One control in a Transport: a glyph and the keys that do the same, in the theme's own colours.
Dimmed while what it does is unavailable, lit while it is the last one used. Where the Transport has
no room for every Chip whole, all of them shed their keys together and none is cut or wrapped. One
Chip says what pressing it does, so continue and pause, like play and pause, are one Chip.
_Avoid_: button, icon, control

### Debugging a running program

**Debug session**:
A program running under a Debug adapter, from launch or attach until it ends. It is laid over Edit
view rather than being a View of its own, because what you do while one is Paused is write code.
One per program, however many sessions the Debug adapter opens beneath it for workers and child
processes: those show as more threads, never as sessions to manage.
_Avoid_: debug mode, debug view, debugger (that is the Debug adapter)

**Launch configuration**:
A named way to start a Debug session — launching a program or attaching to one already running,
such as a service listening on a debug port. Kept in configuration as `[launch.<name>]`, globally
or per project, with the project's winning by name. The Launch box's Create writes one through a
form whose fields are the chosen `[dap.*]` row's own `launch_args` or `attach_args`, so which
arguments an adapter takes is that row's knowledge and never Varde's.
_Avoid_: run configuration, debug profile, target

**Launch form**:
What Create opens over the Launch box: the fields a new Launch configuration is named and filled
in, walked with Tab, and the layer it is written to. A form rather than a sequence of prompts
because the arguments an adapter takes are a list the `[dap.*]` row hands over whole, and which
request is chosen changes which of them there are.
_Avoid_: launch dialog, launch wizard, new launch box

**Run mark**:
The ▶ in the gutter beside something that can be started on its own — a `main`, a test — offering
to run it or debug it without a Launch configuration. What counts as one is configuration, per
language, never Varde's knowledge.
_Avoid_: code lens, run icon, gutter play button

**Debug adapter**:
The child process Varde speaks the Debug Adapter Protocol to — one per language, named in
configuration and never in Varde, the way a language server is.
_Avoid_: debugger backend, debug server, debug engine

**Paused**:
The state of a Debug session whose program is stopped — at a Breakpoint, after stepping, or on an
exception — and so can be inspected and evaluated in. The only state in which Frames, Variables and
the Evaluator mean anything. Several threads can be Paused at once; the one being inspected stays
put when another pauses, and the others are counted in the Frames and on the Transport.
_Avoid_: halted, suspended, stopped (a stopped session has ended)

**Running**:
The state of a Debug session whose program is executing. What the last pause showed stays on
screen, dimmed, so a fast step does not flicker and a long wait does not lose it — and nothing
dimmed is ever mistaken for current.
_Avoid_: continuing, resumed, live

**Waiting**:
The state of an attach Debug session whose program has gone away — restarted or crashed — and which
is listening to attach again, sending its Breakpoints anew when the program answers. A Launch
configuration can opt out; one that attaches does not by default. Ended only by stopping it.
_Avoid_: reconnecting, detached, idle

**Paused line**:
The line the Paused program will run next, in the Frame being inspected.
_Avoid_: current line, execution point, cursor

**Stepping**:
Running a Paused program on by one line, into a call or out of one — a verb only. There is no noun
"a step" in debugging: a Step is a Story's.
_Avoid_: a step, a debug step

**Stepping mode**:
The keyboard state a Space chord leaves behind while a Debug session exists: the debug keys act on
their own, without Space, so a burst of stepping is one key a step. Any other key leaves it and then
does what it always does, so it cannot trap anyone; the Variables' title says while it is on.
_Avoid_: debug mode, hydra, sticky keys

**Chord hint**:
The box a tapped Space opens at once, naming every key that can follow it and what each does, each
one clickable. Drawn from the same list the cheatsheet is, so the two cannot disagree; gone at the
second key or Escape.
_Avoid_: which-key, leader menu, popup

**Frame**:
One call on the Paused program's stack, listed in the Corner under the thread it belongs to.
Choosing one moves the Paused line, the Variables and the Evaluator to that call.
_Avoid_: stack entry, call, Step

**Inline value**:
A variable's value drawn faintly at the end of a line the Paused call has already run, in the Debug
adapter's own words. One that changed since the last pause is drawn highlighted for that pause, so
stepping shows what the line just did. Never drawn on a line the call has not reached.
_Avoid_: inline hint, annotation, value overlay

**Library frame**:
A Frame whose source lies outside the workspace, or that the Debug adapter itself marks as not worth
showing. A run of them is folded into one dimmed row that says how many, and unfolds on request —
never hidden outright, since how much library sits between two of your calls is itself a clue.
_Avoid_: external frame, framework frame, hidden frame

**Breakpoint**:
A line the program pauses at when it reaches it, marked in the gutter. It moves with its line as the
Buffer is edited, and a project remembers it across runs along with the text the line held.
Reaching it pauses only the thread that reached it, unless the Breakpoint says to pause them all.
_Avoid_: stop, marker

**Exception filter**:
A kind of exception the program can be told to pause on — caught, uncaught, a Rust panic, one named
class — offered by the Debug adapter and never invented by Varde. Switched at the top of the
Breakpoint list and remembered per project. Pausing on one puts the exception first in the
Variables.
_Avoid_: exception breakpoint, catchpoint, break on throw

**Unverified breakpoint**:
A Breakpoint the Debug adapter could not bind in the running program — code not loaded, or not the
code on screen — drawn hollow, with the adapter's reason on hover. One it bound to another line is
drawn on that line for the session, and still listed on the line it was set on.
_Avoid_: disabled, broken, Stale (that is Varde's own finding, before any session)

**Conditional breakpoint**:
A Breakpoint that pauses only when its condition holds or its hit count is reached. The condition is
the program's own language, handed to the Debug adapter as written and never read by Varde.
_Avoid_: filtered breakpoint, smart breakpoint

**Logpoint**:
A Breakpoint that prints a message into Program output instead of pausing.
_Avoid_: tracepoint, print breakpoint

**Stale breakpoint**:
A remembered Breakpoint whose line no longer holds the text it was set on. It says so in the
Breakpoint list; it is never quietly re-pointed at whatever line now has its number.
_Avoid_: broken, invalid, orphaned, unverified (that is the Debug adapter's word for another thing)

**Breakpoint list**:
Every Breakpoint in the workspace, one row per line, as a Corner occupant — there with or without a
Debug session, since Breakpoints are set before one starts. A row goes to its line; its Transport
removes one or clears them all.
_Avoid_: breakpoints dialog, breakpoint view

**Watch**:
An expression kept at the top of the Variables and re-evaluated at every pause. One that calls
something is marked as calling, since it runs that call again at every pause; a Hover, by contrast,
never calls anything.
_Avoid_: pinned expression

**Debug group**:
What the Strip shows while a Debug session exists: the Variables beside the Program output, with a
border between them that can be dragged. Every pause brings it forward, as it brings the Frames
into the Corner; ending the session gives both slots back what they held before it.
_Avoid_: debug panel, debug tool window, debug tab

**Program output**:
The debugged program's own terminal, inside the Debug group. It can be hidden to give the Variables
the width without ending it. New output while it is out of sight is marked wherever that is — on
the `Debug` Group tab, and on the Chip that shows it again.
_Avoid_: console, debug terminal, shell (a shell is the user's)

**Pause snapshot**:
What the AI session is handed about a Paused program when asked to be — the Paused line and its
neighbours, the Frames, the Variables as shown, and the exception if one paused it. Pasted into the
AI's prompt and never submitted: values can be real data, so what leaves the machine is the user's
call, made by pressing Enter.
_Avoid_: debug context, AI context, state dump

**Evaluator**:
The floating window that runs a Snippet inside the Paused program, in the chosen Frame. It can be
moved and resized, reopens where a project last left it, never covers the Paused line, and stays
open while you step.
_Avoid_: expression modal, evaluate expression, REPL, console

**Snippet**:
The text in the Evaluator — an expression or a whole block, however much the Debug adapter accepts.
_Avoid_: expression (a Snippet can be many statements), code fragment

**Evaluator output**:
The box beneath the Snippet: what the program printed while the Snippet ran, then its value.
_Avoid_: result pane, console

### Keeping knowledge

**Vault**:
The one folder of markdown the user keeps knowledge in, shared by every workspace and openable as an
Obsidian vault. It holds nothing that names this machine, so it can be handed to someone else whole.
_Avoid_: knowledge base (that is the feature), wiki, notes folder, second brain

**Note**:
One markdown file in the Vault.
_Avoid_: page, document, entry, article

**Topic**:
A folder at the root of the Vault, grouping the Notes that belong together.
_Avoid_: root folder, area, category, project

**Kind**:
A folder inside a Topic naming what sort of Note it holds and so how it is written — a how-to reads
differently from reference documentation. The same knowledge may be written once per Kind it fits.
_Avoid_: type, subfolder, category

**Skill**:
A `SKILL.md` in `~/.varde/ai/skills/` that Varde hands to the AI session when the user picks it,
written so any AI CLI can follow it. One directly in `skills/` is Global; one in
`skills/workflows/` belongs to a Workflow.
_Avoid_: prompt, command, action, recipe

**Workflow**:
A folder under `~/.varde/ai/skills/workflows/` holding Skills meant to be run in order, and the
files they share. Varde ships its Workflows and replaces each whole on update.
_Avoid_: pack, bundle, suite

**Source map**:
The file outside the Vault naming where code lives on this machine, by a name the Vault's Notes may
use. It is the only way a Skill travels from the Vault to source.
_Avoid_: repo list, index, workspace list

**Knowledge view**:
The View that points the tree, Filter, Find and editor at the Vault, leaving the terminal, the AI
pane and everything else on the workspace. Notes are read there and never edited: only a Skill
writes the Vault. Toggling it off returns to the workspace as it was left.
_Avoid_: vault reader, knowledge mode, knowledge pane
