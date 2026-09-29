# Issue tracker: GitHub

Issues and specs for this repo live as GitHub issues on `oyvij/varde-editor`. Use the `gh` CLI for
all operations. The repo is **public**: an issue is published the moment it is created, so nothing
goes in one that could not go in a commit.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body "..."`. Use a heredoc for multi-line bodies.
- **Read an issue**: `gh issue view <number> --comments`, filtering comments by `jq` and also fetching labels.
- **List issues**: `gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'` with appropriate `--label` and `--state` filters.
- **Comment on an issue**: `gh issue comment <number> --body "..."`
- **Apply / remove labels**: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **Close**: `gh issue close <number> --comment "..."`
- **A spec** is an issue of its own; its implementation tickets are sub-issues of it, so one feature
  is still one place to look.

Infer the repo from `git remote -v`; `gh` does this automatically when run inside a clone.

## Pull requests as a triage surface

**PRs as a request surface: no.** _(Set to `yes` if this repo treats external PRs as feature requests; `/triage` reads this flag.)_

When set to `yes`, PRs run through the same labels and states as issues, using the `gh pr` equivalents:

- **Read a PR**: `gh pr view <number> --comments` and `gh pr diff <number>` for the diff.
- **List external PRs for triage**: `gh pr list --state open --json number,title,body,labels,author,authorAssociation,comments` then keep only `authorAssociation` of `CONTRIBUTOR`, `FIRST_TIME_CONTRIBUTOR`, or `NONE` (drop `OWNER`/`MEMBER`/`COLLABORATOR`).
- **Comment / label / close**: `gh pr comment`, `gh pr edit --add-label`/`--remove-label`, `gh pr close`.

GitHub shares one number space across issues and PRs, so a bare `#42` may be either: resolve with `gh pr view 42` and fall back to `gh issue view 42`.

## What this tracker is not

`AGENTS.md` says the cucumber suite is the single source of truth for what is *done*, and that no
hand-written progress checklist may live anywhere, because it drifts. That rule still holds. This
tracker records work that is *intended* — a spec to write, a question to settle, a ticket to pick
up. It never records feature status. Do not mirror scenario or feature completion into issues: run
`cargo test` instead. `docs/example-map.md` tracks the spec; the suite tracks the work; this tracker
tracks the queue.

## The old `.scratch/` tracker

Until this switch, issues lived as markdown files under `.scratch/<feature-slug>/`, deleted once
their feature shipped. `docs/example-map.md` still names those paths for the reasoning behind
shipped features; they exist only in git history — `git log --all -- <path>` finds the commit to
`git show`. Never recreate `.scratch/`: new work goes to GitHub.

## Burning the queue unattended

`scripts/burn.sh` works through every open `ready-for-agent` issue with no one watching: one fresh
`claude -p /implement` session per issue, each squashed into one commit on the `burn` branch on top
of the last, pushed and gathered in one draft PR. A red issue is dropped from the branch and moved to
`ready-for-human`; sub-issues of the debugger spec (#45) are skipped, since they belong on
`debugger`. `--dry-run` prints the order. Merge the PR whole, or land a prefix with
`git push origin <sha>:main`.

## Closing a ticket

When the work is merged and the suite is green:

- Tick the `- [ ]` acceptance criteria in the issue body you actually implemented and verified. Tick
  a box only once its behaviour is covered by a passing test; never tick ahead of the suite.
- Close with a comment carrying what the next reader needs — the decision taken, the binding chosen,
  the thing that turned out to be wrong. `/implement` does not do this; it implements and commits.
  The rationale is the part no diff carries.

A closed issue is a queue state, never a claim about the code — `cargo test` is still the only thing
that says a feature works. The boxes are not a second copy of that claim: they record *which* of a
ticket's criteria this pass covered, for a ticket only partly done or picked up by someone else.

## Do not use git worktrees for tickets

Issue 10 was implemented in a worktree under `.claude/worktrees/`, and renaming this repo's folder
broke the registration — git stores worktree paths absolutely — so its branch went unmerged and
invisible while `main` kept the old code. The finished work had to be recovered by reading a dead
process's arguments. Nothing in the skills asks for a worktree: `/implement` says "commit your work
to the current branch". Work tickets on a branch in this checkout.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --comments`.

## Wayfinding operations

Used by `/wayfinder`. The **map** is a single issue with **child** issues as tickets.

- **Map**: a single issue labelled `wayfinder:map`, holding the Notes / Decisions-so-far / Fog body. `gh issue create --label wayfinder:map`.
- **Child ticket**: an issue linked to the map as a GitHub sub-issue (`gh api` on the sub-issues endpoint). Where sub-issues aren't enabled, add the child to a task list in the map body and put `Part of #<map>` at the top of the child body. Labels: `wayfinder:<type>` (`research`/`prototype`/`grilling`/`task`). Once claimed, the ticket is assigned to the driving dev.
- **Blocking**: GitHub's **native issue dependencies**, the canonical, UI-visible representation. Add an edge with `gh api --method POST repos/<owner>/<repo>/issues/<child>/dependencies/blocked_by -F issue_id=<blocker-db-id>`, where `<blocker-db-id>` is the blocker's numeric **database id** (`gh api repos/<owner>/<repo>/issues/<n> --jq .id`, _not_ the `#number` or `node_id`). GitHub reports `issue_dependencies_summary.blocked_by` (open blockers only, the live gate). Where dependencies aren't available, fall back to a `Blocked by: #<n>, #<n>` line at the top of the child body. A ticket is unblocked when every blocker is closed.
- **Frontier query**: list the map's open children (`gh issue list --state open`, scoped to the map's sub-issues / task list), drop any with an open blocker (`issue_dependencies_summary.blocked_by > 0`, or an open issue in the `Blocked by` line) or an assignee; first in map order wins.
- **Claim**: `gh issue edit <n> --add-assignee @me`, the session's first write.
- **Resolve**: `gh issue comment <n> --body "<answer>"`, then `gh issue close <n>`, then append a context pointer (gist + link) to the map's Decisions-so-far.
