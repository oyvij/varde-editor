# Reading aloud

A **Reading** is the Selection spoken by a synthesizer. Select a passage in a markdown file, type
`:read`, and a voice reads it while a mark in the editor follows along. It does not replace reading.
It runs alongside it: in a long document your attention drifts, and a voice sets a pace your eyes
can follow.

A Reading covers the Selection and nothing else. You cannot read from the cursor or read a whole
file, and Varde does not fall back to the paragraph under the cursor. Select, play, and when it ends,
select the next passage. This is deliberate, because the passage worth hearing is the one you picked.

Varde has no voice built in. The synthesizer is an ordinary program on your `PATH` (`piper` by
default), the voice is a model file on your disk, and an ordinary player plays the sound (`afplay`
on macOS, `aplay` on Linux). See [Installing](#installing-a-voice) below and
[../install.md](../install.md).

## What can be read

| Condition | If it does not hold |
|---|---|
| The current buffer is a markdown file | Refused: `not-markdown`. Varde reads nothing else aloud, and does not draw the Transport on other files. |
| There is a Selection | Refused: `nothing-selected`. |
| `speech.voice` names a model file that is on disk | Refused: `no-voice`, and Tools opens on the speech synthesizer row. `i` installs it, the same as taking it in Tools. |
| The synthesizer is running | Refused: `no-synthesizer`, with the same offer. |
| A player is configured and on `PATH` | Refused: `no-player`, and Tools opens on the player row. Nothing installs a player. |

A Selection here is the same thing copying copies: a mouse drag, or a keyboard extend with Shift and
an arrow. Two cases surprise people:

- **A project search leaves its hit as the Selection.** Press `Ctrl+F`, pick a hit, run `:read`, and
  the voice says the matched word, not the passage around it. Select the passage first.
- **A Preview has no linewise selection.** A markdown file opens in its rendered Preview. There, a
  drag or a Shift-extend picks rendered rows and you can read them, but `V` and Shift+Down do
  nothing. To select whole lines, run `:preview` to switch to Source first.

The voice receives the passage as prose, not as markdown source: no `#`, no `*`, no link URL. It
reads a heading as its words and a link as its text. The Preview strips the same markup, so Preview
and Source read the same. Varde does **not** skip code blocks, so stop a Reading by hand if it reaches
one.

## Starting, pausing, skipping, stopping

Every control is a `:` command, and each one also has a Chip on the **Transport**. The Transport is
the row of clickable Chips on the editor's top border, drawn only over a markdown buffer. Each Chip
shows a glyph and the command that does the same thing, so the Transport shows you its own keys. On
a border too narrow for all of them, every Chip drops its command at once and keeps its glyph.
Previous, next and stop are dimmed while nothing is being read, and the last control you used, by
click or by command, stays lit until you use another. The cheatsheet lists them on one row:
`:read :pause :next :prev :stop :speed`. See [editing.md](editing.md) for the rest of the editor's
keys.

| Command | Does |
|---|---|
| `:read` | Starts a Reading of the Selection. Starting one while another is playing **replaces** it. Nothing queues. |
| `:pause` | Toggles: pauses a Reading that is playing, and resumes a paused one from exactly where it stopped. On the Transport this is the play/pause control. Pressed with no Reading playing, it starts one over the Selection, so you only need to select and press play. |
| `:next` | Skips to the next Utterance. Past the last one, the Reading ends. |
| `:prev` | Goes back one Utterance. At the first, it stays there and you hear it again. |
| `:stop` | Ends the Reading. Nothing is marked and no sound plays. |
| `:speed <n>` | Sets the pace for the **next** Reading (see below). |

An **Utterance** is one sentence of speech. `:next` and `:prev` move by Utterances, and the mark on
screen shows the current one. Varde splits sentences by the Unicode rules, so a colon does not end
one and neither does the `.` in `1.0` or `e.g.`.

### The pauses you hear

Varde builds a Reading as one continuous audio stream with real silence in it, not as a player per
sentence. The gaps are part of the spoken passage:

| Between | Silence |
|---|---|
| Two sentences in the same block | 550 ms |
| The last sentence of a block and the next block | 1000 ms |

A block is what the Preview separates (a paragraph, a heading, a code fence), plus each list item.
List items are not separated on screen, but a listener should hear them apart.

### The mark

While a Reading plays, Varde marks the Utterance being spoken with a marker in the gutter and a dim
wash behind the lines, never an inversion. It follows the sound rather than the last key you
pressed, so skipping moves it because skipping moves the sound. Varde draws it on the buffer the
passage came from, so switching to another file does not put the mark on the wrong text. When the
Reading ends, nothing is marked.

### Speed

`speed` is a multiplier, and higher is faster. `1.0` is the voice's own pace, `1.25` is a quarter
faster, and `0.9` is a little slower. `:speed <n>` takes any positive number. Varde refuses zero,
negatives and anything that is not a number, rather than clamping them.

A speed change **applies to the next Reading**, not the one playing. Varde sets the pace when it
builds the stream, and changing the pace mid-sentence would repeat words you had just heard. The
Transport's speed control steps through `0.75`, `1.0`, `1.25`, `1.5`, `2.0` and wraps back to the
start. Use `:speed` for a pace between those steps. `speech.speed` in the config holds the same
value, so you can set a preferred pace once.

## When nothing is installed

Reading does nothing until a synthesizer and a voice exist on the machine, and it tells you so
rather than staying silent, because silence is also what success sounds like before the first word.
Each missing piece refuses by name (`no-voice`, `no-synthesizer`, `no-player`). Varde **types the
install command for your operating system onto the terminal's input line and does not run it**. Read
it, edit it if your package manager differs, then press Enter yourself. Varde executes, downloads
and writes nothing. A row with no install command for this OS refuses and offers nothing.

Varde checks in the order you fix things: the voice first, because a synthesizer with no model to
load never starts.

### Installing a voice

A voice is two things: the **synthesizer binary** and a **model file** it loads. Take the speech
synthesizer row in Tools, or press `i` when Varde refuses a Reading for a missing piece. The shipped
install command runs in the shell pane. It installs `piper` with `uv` and fetches the
`en_US-bryce-medium` model into `~/.varde/voices/`: an `.onnx` file with its `.onnx.json` beside it,
about 61 MB, public domain. When it exits 0, Varde writes the row's `configures.voice` into
`~/.varde/config.toml`:

```toml
# ~/.varde/config.toml
[speech]
voice = "~/.varde/voices/en_US-bryce-medium.onnx"
```

`voice` ships blank on purpose. It is a path on your disk, and an invented one would look configured
and not work. The install fills it in, and only when it is blank, so Varde never overwrites a voice
you chose. A `~` at its start is your home directory. A `voice` naming a file that is not there gives
`no-voice`, the same as a blank one.

Any other piper voice works the same way: download its `.onnx` and `.onnx.json` and point `voice` at
the `.onnx`. You can name any other synthesizer in `speech.command` and `speech.args`, as long as it
reads text on stdin and writes its audio where `${dir}` says. Varde never checks which one it is.

Varde starts the synthesizer when the first markdown buffer opens, because otherwise loading a voice
takes long enough to miss the first word. It then stays running and holds a couple of hundred
megabytes.

## The `[speech]` table

All of these live in `~/.varde/config.toml` or the project's `.varde/config.toml`. See
[configuration.md](configuration.md) for how the two layer. The seeded project file lists `command`,
`args`, `voice` and `speed` commented out.

| Key | Shipped default | Meaning |
|---|---|---|
| `command` | `"piper"` | The synthesizer, found on `PATH`. |
| `args` | `["--model", "${voice}", "--length-scale", "${scale}", "--noise-w-scale", "1.0", "--output_dir", "${dir}"]` | Its arguments. `${voice}` is the `voice` row, `${scale}` the reciprocal of `speed` (the synthesizer scales duration, so it runs backwards; you never write the inverted number), `${dir}` where Varde writes the stream. `--noise-w-scale 1.0` was chosen by ear over the model's 0.8. |
| `voice` | `""` | Path to the model file; a leading `~` is your home directory. Blank until the install fills it in. |
| `speed` | `1.0` | Multiplier, higher is faster. What `:speed` changes for the session. |
| `player.macos` | `"afplay"` | What plays the stream on macOS. |
| `player.linux` | `"aplay"` | What plays it on Linux (`alsa-utils`). |
| `player.windows` | none | None shipped: Windows has no command-line wav player that runs without its own shell. |
| `install.macos`, `install.linux` | `uv tool install piper-tts && mkdir -p ~/.varde/voices && curl …` | Runs in the shell pane when you take the speech row in Tools. |
| `configures.voice` | `"~/.varde/voices/en_US-bryce-medium.onnx"` | Written into `voice` once the install exits 0, if `voice` is blank or absent in `~/.varde/config.toml`. |
| `install.windows` | none | None shipped. |

## Where the audio goes

A Reading writes its stream to `~/.varde/tmp/`, outside every workspace, plays it, and deletes it.
Nothing appears in the file tree, `git status` is unchanged and a project search finds nothing new.
Varde removes the file when the player exits and again when Varde exits. It also clears the
directory on the next start in case a crash skipped both. Everything in that directory belongs to
Varde, so it can delete all of it. It is the one thing Varde writes to `~/.varde/` that is not
configuration or a review.

For the reasoning, see `docs/adr/0013-a-voice-is-an-installed-binary.md` on why the voice is a
program you install rather than a library Varde links, and
`docs/adr/0014-scratch-audio-lives-outside-the-workspace.md` on why the stream is not in the
project's `.varde/` or the OS temp directory.
