#!/usr/bin/env bash
# Implements every open `ready-for-agent` issue, one fresh headless Claude session per issue, as one
# squashed commit per issue on a single branch (`burn`), each built on the one before it. Green
# issues are pushed and collected in one draft PR against main; red ones are dropped from the branch
# and handed to a human. Merge the PR whole, or up to a commit: `git push origin <sha>:main`.
# Each session's steps stream to the terminal and to burn.log as they happen.
#
#   scripts/burn-stream.sh              run the whole queue
#   scripts/burn-stream.sh --dry-run    print the order it would take and stop
#   scripts/burn-stream.sh 107 111      run only these issues, in the queue's order
#
# Environment: BURN_BRANCH (burn), BURN_TIMEOUT per issue (90m), BURN_PERMISSION_MODE (auto),
# BURN_EXCLUDE_PARENTS — specs whose sub-issues belong on another branch (45, the debugger),
# BURN_MAX_FAILURES in a row before giving up (3).
set -euo pipefail

BRANCH=${BURN_BRANCH:-burn}
TIMEOUT=${BURN_TIMEOUT:-90m}
PERMISSION_MODE=${BURN_PERMISSION_MODE:-auto}
EXCLUDE_PARENTS=${BURN_EXCLUDE_PARENTS:-45}
MAX_FAILURES=${BURN_MAX_FAILURES:-3}

DRY_RUN=0
ONLY=()
for arg in "$@"; do
  case $arg in
    --dry-run) DRY_RUN=1 ;;
    [0-9]*) ONLY+=("$arg") ;;
    *) echo "usage: $0 [--dry-run] [issue...]" >&2; exit 2 ;;
  esac
done

ROOT=$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")
WT=$ROOT/.claude/worktrees/$BRANCH
LOGS=$ROOT/.claude/worktrees/$BRANCH-logs
REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)

command -v jq >/dev/null || { echo "burn needs jq to stream the sessions" >&2; exit 1; }

# One line per thing the agent says or runs. `fromjson?` skips a line that is not JSON rather than
# ending jq, whose exit would take the session down with it through a broken pipe.
PRETTY='fromjson? | select(.type=="assistant") | .message.content[]?
  | if .type=="text" then $chat + "💬 " + (.text | gsub("\n"; " ") | .[0:200]) + $off
    elif .type=="tool_use" then $tool + "🔧 " + .name + " "
      + ((.input.command // .input.file_path // .input.description // .input.skill // "") | tostring | gsub("\n"; " ") | .[0:150]) + $off
    else empty end'

# Chat green, tool calls dim yellow, the loop's own lines bold magenta; NO_COLOR=1 turns them off.
if [[ -n ${NO_COLOR:-} ]]; then
  CHAT='' TOOL='' LOOP='' DIM='' OFF=''
else
  CHAT=$'\e[32m' TOOL=$'\e[2;33m' LOOP=$'\e[1;35m' DIM=$'\e[2m' OFF=$'\e[0m'
fi

log() { mkdir -p "$LOGS"; printf '%s[%s] %s%s\n' "$LOOP" "$(date +%H:%M:%S)" "$*" "$OFF" | tee -a "$LOGS/burn.log"; }

# --- the queue -------------------------------------------------------------------------------

excluded=" "
for parent in $EXCLUDE_PARENTS; do
  excluded+="$(gh api --paginate "repos/$REPO/issues/$parent/sub_issues" --jq '.[].number' | tr '\n' ' ')"
done

queue=()
for n in $(gh issue list -R "$REPO" -l ready-for-agent --state open --limit 500 --json number \
             --jq 'sort_by(.number)[].number'); do
  [[ $excluded == *" $n "* ]] && { log "skip #$n: sub-issue of an excluded spec"; continue; }
  if ((${#ONLY[@]})) && [[ " ${ONLY[*]} " != *" $n "* ]]; then continue; fi
  queue+=("$n")
done

# Open issues that must land first: GitHub's own blocked-by links, and "Blocked by #n" in the body.
blockers() {
  {
    gh api "repos/$REPO/issues/$1/dependencies/blocked_by" --jq '.[] | select(.state=="open") | .number'
    for b in $(gh issue view "$1" -R "$REPO" --json body -q .body | grep -oiP 'blocked by:?\s*#\K\d+'); do
      if [[ $(gh issue view "$b" -R "$REPO" --json state -q .state) == OPEN ]]; then echo "$b"; fi
    done
  } | sort -u
}

declare -A done_here=()
# The next issue whose blockers are all closed or done in this run; lowest number first.
next_issue() {
  local n b ok
  for n in "${queue[@]}"; do
    [[ -n ${done_here[$n]:-} ]] && continue
    ok=1
    for b in $(blockers "$n"); do [[ -n ${done_here[$b]:-} ]] || ok=0; done
    if ((ok)); then echo "$n"; return 0; fi
  done
  return 0
}

if ((DRY_RUN)); then
  for ((i = 0; i < ${#queue[@]}; i++)); do
    n=$(next_issue)
    [[ -z $n ]] && break
    done_here[$n]=1
    echo "#$n  $(gh issue view "$n" -R "$REPO" --json title -q .title)"
  done
  for n in "${queue[@]}"; do [[ -z ${done_here[$n]:-} ]] && echo "#$n  blocked: $(blockers "$n" | tr '\n' ' ')"; done
  exit 0
fi

# --- the branch ------------------------------------------------------------------------------

git -C "$ROOT" fetch origin
# Resume an unmerged burn branch; once main contains it, start over from main.
base=origin/main
if git -C "$ROOT" rev-parse -q --verify "origin/$BRANCH" >/dev/null &&
   ! git -C "$ROOT" merge-base --is-ancestor "origin/$BRANCH" origin/main; then
  base=origin/$BRANCH
fi
[[ -d $WT ]] || git -C "$ROOT" worktree add --detach "$WT" "$base"
cd "$WT"
if [[ -n $(git status --porcelain) ]]; then
  echo "$WT has uncommitted changes; clean it up before burning" >&2
  exit 1
fi
git switch -C "$BRANCH" "$base"
log "burning ${#queue[@]} issue(s) on $BRANCH from $base"

ensure_pr() {
  [[ -n $(gh pr list -R "$REPO" --head "$BRANCH" --state open --json number -q '.[].number') ]] && return
  gh pr create -R "$REPO" --draft --base main --head "$BRANCH" \
    --title "Agent burn: ready-for-agent issues" \
    --body "$(printf '%s\n\n%s\n\n%s\n' \
      "One commit per issue, oldest first, each built on the one before it. Each commit passed \`cargo test\` on top of its predecessors." \
      "Merge whole, or land a prefix with \`git push origin <sha>:main\`. Squash only if merging the whole branch." \
      "🤖 Generated with [Claude Code](https://claude.com/claude-code)")"
}

# The issue's run produced nothing usable: take the branch back and hand the issue to a human.
reject() {
  local n=$1 why=$2
  git reset -q --hard "$prev"
  git clean -qfd
  gh issue edit "$n" -R "$REPO" --remove-label ready-for-agent --add-label ready-for-human >/dev/null
  gh issue comment "$n" -R "$REPO" --body "An unattended agent run could not finish this: $why. Nothing was pushed for it; the run's log is on the machine that ran it (burn-$n.*)." >/dev/null
  log "#$n rejected: $why"
}

failures=0
landed=()
while :; do
  n=$(next_issue)
  [[ -z $n ]] && break
  done_here[$n]=1
  prev=$(git rev-parse HEAD)
  title=$(gh issue view "$n" -R "$REPO" --json title -q .title)
  log "#$n started: $title"

  status=0
  timeout "$TIMEOUT" claude -p "/mattpocock-skills:implement GitHub issue #$n on $REPO — read it with \`gh issue view $n --comments\`.
Implement this issue only. You are running unattended: nobody will answer a question, so make the reasonable call and note it in the commit message.
Commit to the current branch, bumping the version as AGENTS.md says. Do not push, switch branches, open a PR, or edit, comment on or close any issue — the loop that started you does that." \
    --permission-mode "$PERMISSION_MODE" --output-format stream-json --verbose </dev/null 2>"$LOGS/burn-$n.err" |
    tee "$LOGS/burn-$n.jsonl" | jq --unbuffered -rR --arg chat "$CHAT" --arg tool "$TOOL" --arg off "$OFF" "$PRETTY" |
    sed -u "s/^/$DIM#$n$OFF /" | tee -a "$LOGS/burn.log" ||
    status=${PIPESTATUS[0]}

  commits=$(git rev-list --count "$prev..HEAD")
  # A session that died without doing anything is a broken tool (a usage limit, an auth failure),
  # not a hard issue: stop rather than mark every remaining issue as one a human must take.
  if ((status != 0 && status != 124 && commits == 0)); then
    git reset -q --hard "$prev"
    log "#$n: claude exited $status without committing; stopping (see $LOGS/burn-$n.err). The issue keeps its label."
    break
  fi

  git reset -q --hard HEAD # whatever the session left uncommitted is not part of its work
  git clean -qfd
  if ((commits == 0)); then
    reject "$n" "the session ended ($([[ $status == 124 ]] && echo "timed out after $TIMEOUT" || echo "exit $status")) without committing"
  elif ! cargo test >"$LOGS/burn-$n.test" 2>&1; then
    reject "$n" "\`cargo test\` failed on top of the session's commits"
  else
    [[ $(git diff "$prev" HEAD -- Cargo.toml) == *'+version'* ]] || log "#$n: warning — Cargo.toml version not bumped"
    body=$(git log --format='%B' "$prev..HEAD" | grep -v '^Co-Authored-By:' | cat -s)
    git reset -q --soft "$prev"
    git commit -q -F - <<EOF
$title

Closes #$n

$body

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
    git push -q origin "HEAD:refs/heads/$BRANCH"
    ensure_pr
    gh issue edit "$n" -R "$REPO" --remove-label ready-for-agent >/dev/null
    gh issue comment "$n" -R "$REPO" --body "Implemented unattended as $(git rev-parse --short HEAD) on \`$BRANCH\`; closes when that reaches main." >/dev/null
    landed+=("$n")
    failures=0
    log "#$n landed as $(git rev-parse --short HEAD)"
    continue
  fi

  failures=$((failures + 1))
  if ((failures >= MAX_FAILURES)); then
    log "$failures issues in a row failed; stopping so a systematic problem does not burn the whole queue"
    break
  fi
done

log "landed ${#landed[@]}: ${landed[*]:-none}"
git log --oneline --reverse "origin/main..HEAD" | tee -a "$LOGS/burn.log"
