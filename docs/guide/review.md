# Review

Review view is where you read what just changed before anyone commits it. Usually that is what the
AI in the right-hand pane just wrote. It shows only the files git reports as changed, each as a
diff, and lets you annotate ranges of lines with `ISSUE`, `NOTE`, `SUGGESTION` or `COMMENT`.
Submitting the review writes it to a file and sends it into the AI pane, so the AI starts acting on
it the moment you confirm.

The view is for reading. You cannot type over anything in a diff. `e` takes you to the real file when
you want to change it yourself.

## Entering Review view

Open the palette (`Ctrl+Space`, or `Ctrl+Ctrl`; `Esc Esc` from inside a hosted pane) and press
`r`. Press `e` in the palette to go back to Edit view. See [Getting around](getting-around.md) for
the palette and focus.

On entry the changed-file list replaces the file tree in the left-hand pane. Varde selects the first
changed file, starts loading its diff in the centre, and puts the keyboard on the list. You switched
here to look at changes, so it shows you one straight away.

Leaving Review view clears the diff, so the editor goes back to the file you were editing rather
than a read-only diff of it.

## What is listed

Everything uncommitted against `HEAD`:

- modified files, whether or not the change is staged;
- untracked files;
- never anything `.gitignore` covers.

The view shows two different empty states, because they mean different things:

- the folder is **not a git repository**, and the view opens and says so;
- the working tree is **clean**, with a different message, and there is nothing to review.

## Reading a diff

The diff is a unified diff, read-only, with the code on both sides syntax-highlighted as the real
file would be. Added and removed rows have a muted background so highlighted tokens stay readable
on top of it.

The changed-file list takes the same keys the file tree does. Moving the selection onto a file shows
its diff, and the keyboard stays on the list so you can keep browsing. Choosing a file with `Enter`
or a click shows the same diff and moves the keyboard into it, because commenting comes next and
only the editor's keys can do it.

| Key | In the list |
|---|---|
| `Up` `Down` | move between changed files, showing each diff |
| `Enter` | show the selected file's diff and move focus into it |
| click a row | the same, and focus follows the click |

| Key | In the diff |
|---|---|
| `j` `k` | move down and up a row |
| `l` `h` | slide the diff right and left, for a long line |
| `0` | slide back to the start in one keypress |
| `V` | start selecting lines |
| `c` | open the comment box on the selected lines |
| `e` | open this file in Edit view, at the real file |
| `Ctrl+P` `Ctrl+N` | jump back and forward through cursor history |
| `Ctrl+C` / `Cmd+C` | copy a dragged selection |
| `Alt+h` `Alt+j` `Alt+k` `Alt+l` | move focus between panes |

The diff only slides sideways when you ask. It has no column cursor to follow, so `l` and `h` move
the window, and moving up and down never slides it. Sliding right stops at the longest row rather
than running on into empty space.

To copy text out of a diff, drag over it with the mouse and press `Ctrl+C` or `Cmd+C`. A drag over
the *code* selects. A drag in the *gutter*, the line-number strip, does something else, described
below.

## Commenting

A comment covers a range of lines in one file. Comments use the **new** file's line numbers, because
that is the code the AI has to change. A comment on a removed line points at where it was.

There are two ways to pick the range:

- **Keyboard.** Put the cursor on a line in the diff, press `V`, extend with `j` and `k`, then
  press `c`.
- **Mouse.** Drag down the gutter from the first line to the last. The comment box opens when you
  let go.

Either way the comment box opens over the diff and asks for a type first.

### The four kinds

| Letter | Kind | What it means |
|---|---|---|
| `i` | `ISSUE` | Something that must change. The only kind that blocks. |
| `n` | `NOTE` | Context for the AI; not a request. |
| `s` | `SUGGESTION` | A better way you can see; the AI may take it or not. |
| `c` | `COMMENT` | Anything else worth saying about these lines. |

Only those four letters pick a type. Any other key does nothing, so a stray keystroke cannot choose
one for you. `Esc` here closes the box and records nothing.

### Writing the body

Once you pick a type, the box is a small text editor. Type the body and move with the arrows. `Alt`
with `Left`/`Right` moves a word, `Alt+Backspace` deletes a word, and a paste keeps all its lines.
`Enter` starts a new line rather than filing the comment, so multi-line bodies work as you'd expect.
The box's footer names the three keys that are not text:

| Key | Action |
|---|---|
| `Ctrl+S` | file the comment |
| `Esc` | discard it and close the box |
| `Ctrl+Z` | undo the last edit. A pasted stack trace or a deleted word undoes in one key |

Varde draws a filed comment in the diff against the last line of its range and tells you it was
added. Every comment also records the revision of the file that was on screen when you made it, so it
still points at what you saw even after the AI rewrites the file.

## Submitting

Type `:submit` and press `Enter`. Varde asks you to confirm before sending anything, because sending
clears whatever the AI's command line currently shows, which can be a half-written message of yours.

| Key | In the confirmation |
|---|---|
| `Enter` or `y` | submit |
| `Esc` or `n` | decline. Nothing is sent, the AI's prompt is untouched, and your comments stay |

Varde refuses to submit an empty review and tells you why.

### The verdict

The review's contents decide which of two verdicts it carries:

- `changes-requested` if the review contains at least one `ISSUE`;
- `commented` if it contains only `NOTE`s, `SUGGESTION`s and `COMMENT`s.

### Where it goes

Confirming does three things.

1. **Writes the review** to `.varde/reviews/NNNN.json` in the project, numbered on from the last
   one. Varde keeps the last fifty and deletes older ones on submit. From a Bare workspace (`varde`
   with no folder) the file goes to `~/.varde/reviews/` instead, so it outlives the session. See
   [Stories](stories.md#a-bare-workspace).
2. **Sends it into the AI pane** and submits the prompt. The prompt summarises every comment with
   `file:from-to`, the type and the body, plus the path of the file just written. You do not have to
   press `Enter` in the AI pane. When the AI's CLI supports it, the whole review arrives as a single
   paste with its line breaks intact.
3. **Clears your comments**, ready for the next pass over whatever the AI does next.

The file looks like this:

```json
{
  "verdict": "changes-requested",
  "comments": [
    {
      "file": "src/tree.js",
      "from_line": 14,
      "to_line": 18,
      "type": "ISSUE",
      "body": "unquoted path",
      "revision": "<blob id of the file as reviewed>"
    }
  ]
}
```

A comment made while walking a Story also records the story and step it was made against. One made
in Review view records neither. Because the review is a file, it works with any AI CLI that can read
a file.

### When no AI session is running

If the AI pane is empty, confirming starts the configured AI command first (`[ai] command`, see
[Configuration](configuration.md)). Varde holds the review until that CLI has printed its prompt and
is ready for input, because a CLI drops what you type at it too early. If the CLI exits before it is
ready, or you replace it with `:ai!` in the meantime, Varde drops the queued review rather than
handing it to the next session. The file on disk still has it. See [AI pane](ai-pane.md) for how
sessions start and get replaced.

## Risk in Review view

Entering Review view also measures [Risk](risk.md) for exactly the files under review. It compares
each file as it is now against the revision the diff starts from. The tree pane's top border shows
the **delta**, how much the change moved the figure, rather than the workspace's Risk count, and it
marks a change that made things worse. Leaving the view puts the workspace's count back.

If the Risk list is open, it lists the Functions in the changed files with their figures and deltas,
so you can see which part of the change added the risk. No Function from outside the change appears
there. A changed file in a language the analyser does not handle contributes nothing, and Varde never
counts it as an improvement.

The Risk pane's own action starts a **Refactor loop over the reviewed files**. It is the same loop as
the workspace one with a different file set, so the guarantees are the same. The tests must pass, the
figure must fall, and no other metric may rise. Varde restores a failed pass from its snapshot,
commits nothing, and touches no file outside the review. The same action in the same place stops it.
[Risk](risk.md#the-refactor-loop) has the details.

## What Review view does not do

- It does not let you edit a diff. `e` opens the real file.
- It does not show committed history, only what is uncommitted against `HEAD`.
- It does not read the AI's reply. The AI's response appears in its own pane, and nothing in the
  review is marked "addressed" automatically.
- It does not keep a submitted review on screen. Once sent, the comments clear, and the numbered file
  is the record.
- It never edits your `.gitignore` or any other git file.

## See also

- [Stories](stories.md): ask the AI to narrate the change you are reviewing, and comment from inside
  a Story.
- [Risk](risk.md): the figure on the border and the loop that lowers it.
- [AI pane](ai-pane.md): the session a submitted review goes to.
- [Getting around](getting-around.md): the palette, panes and focus.
