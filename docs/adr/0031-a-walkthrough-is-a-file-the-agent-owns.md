# A Walkthrough is a file the agent owns

A Walkthrough has an agent plan a change and guide the developer through writing it (issue #116).
Something has to hold the plan, the order of its Waypoints, the agent's verdicts on what the
developer wrote, where a revised plan resumes, and whether the session is over — and something has
to put files back when the developer drops the attempt.

**The plan is one file, `walkthrough.json`, and the agent owns everything in it. Varde only reads
it.** It lives in the workspace's `.varde/`, or in the Sidecar for a Bare workspace (ADR 0016). One
Walkthrough exists at a time. The agent writes plan order, every verdict, the `checkpoint`, the
`status` and every revert of a source file; Varde watches the file, reloads it on every change, lands
on its `checkpoint`, and never writes it.

```json
{
  "version": 1,
  "status": "active",
  "checkpoint": "w1",
  "waypoints": [
    {
      "id": "w1",
      "file": "src/guide.rs",
      "line": 12,
      "kind": "code",
      "text": "Parse the plan file into a Plan, refusing a Waypoint with no id",
      "verdict": { "state": "failing", "message": "An empty id is still accepted" },
      "ai": false
    },
    {
      "id": "w2",
      "file": "src/lib.rs",
      "line": 40,
      "kind": "context",
      "text": "`update` is where the plan arrives; this is why it lands here.",
      "verdict": null,
      "ai": false
    }
  ]
}
```

`status` is `active`, `completed` or `ended`. `kind` is `code` or `context`. `text` is the
instruction Varde writes as a comment on a code Waypoint, or the Markdown a context Waypoint shows.
`verdict` is `null` until the agent has evaluated, then `lgtm` or `failing` with a message. `ai` says
the agent wrote the snippet. A Waypoint's `line` is 1-based, and its file may not exist yet. Plan
order is array order: Varde never sorts.

Every request Varde makes is one line naming the shipped `walkthrough` Skill (ADR 0026), the action
— `plan`, `evaluate`, `adjust`, `write` or `revert` — the plan file and the current Waypoint, pasted
into the AI pane or given on stdin to a configured background command. Varde reads nothing the agent
prints (ADR 0004, ADR 0006); the file is the only thing that comes back.

## Varde never restores a file

On Revert the agent restores every file the session touched and then sets `status` to `ended`. On an
adjustment it reverts what the new approach replaces, rewrites the plan and names the `checkpoint`.
Varde snapshots nothing and restores nothing, and before every request it saves every buffer the
Walkthrough touches, so the agent never works on stale files.

## Considered options

- **Varde snapshots each touched file and restores it on Revert**, as the Refactor loop's Gate does
  (ADR 0010). Recommended, and rejected by the user. The Gate is Varde's because Varde measures an
  Iteration and the session's report is only belief; a Walkthrough has nothing for Varde to measure,
  and an adjustment ("move these functions to a new file") needs a revert that knows the new
  approach, which only the agent does. A snapshot would also have to decide which of the developer's
  own edits since landing to keep — a merge with no right answer.
- **A Story set variant.** Rejected: a Story set is authored once and read, is refused whole when it
  is malformed, and dies with its Range. A plan is rewritten through the session, keeps the last
  valid version when a rewrite is broken, and is remembered across a restart. Story code is reused
  where it fits — the Inquiry box, the Preview renderer, Site checking and the two fix rounds.
- **Varde owns verdicts or order**, sorting Waypoints by file or line, or deciding LGTM itself.
  Rejected: build order is a judgement about the change, and a verdict is a judgement about the code;
  both are what the agent is there for.

## Consequences

A plan that does not parse, or a Waypoint whose file or line is out of range, leaves the last valid
plan in use, and Varde sends one fix request naming the failing Waypoints, for up to two rounds, as
Stories do. Because Varde never writes the file, the agent can always rewrite it without merging
with Varde's changes; what Varde must remember on its own — which comments it inserted, where each
Waypoint's line has moved to under the developer's edits, the last Waypoint visited — it keeps in
its own state. A rewrite of the file by the agent replaces those line positions.

Neither driver has provider-specific code. Resuming a provider's conversation is the user's choice,
made by adding flags to `walkthrough.command`; each request names the plan file and the current
Waypoint, so a fresh process needs nothing else.
