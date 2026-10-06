@AGENTS.md

## Agent skills

### Issue tracker

Issues live as GitHub issues on `oyvij/varde-editor`, driven with the `gh` CLI; the old `.scratch/` tracker survives only in git history. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical roles, unrenamed, applied as GitHub labels. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: `CONTEXT.md` + `docs/adr/` at the repo root, both created lazily. See `docs/agents/domain.md`.

## Running long commands

Run the test suite in the foreground with a long timeout (up to 10 minutes), or in the background
and wait for its completion notification. Never write a `pgrep`/`sleep` loop to wait for it:
`pgrep -f "cargo test"` matches the loop's own command line, so the loop never ends — an
`/implement` run sat on one for minutes after the suite had already passed.

**One full run per ticket.** The full suite takes 5–10 minutes. While working, run only the tests
the change touches: `cargo test --lib <filter>` and `cargo test --test cucumber -- --name "<regex>"`.
Run the full `cargo test` once, last, after merging the branch you will land on. No baseline run
before starting, and no second full run after a clean merge of a branch that already passed on the
tip. Spec #150 ran it four to eight times per ticket and lost a day to waiting. This holds for
every agent: a brief or prompt written for subagents must not ask for more full runs than this.
When a spec's red scenarios are merged before its tickets, run the suite once, save the failing list
to a file, and point implementers at it instead of asking each one for a baseline.
