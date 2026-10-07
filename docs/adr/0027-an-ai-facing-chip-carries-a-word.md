# An AI-facing Chip carries a word

Amends `0022-every-action-has-a-chip.md`, which says a Chip is a glyph and its keys, no word.

The inject Chip followed that rule and was not used: one Nerd Font arrow on the AI pane's border,
the pane the text goes *to* rather than the one it comes *from*, too small to find and reading as
nothing on a terminal without the font. A Chip that hands something to the AI starts an exchange
the reader will go on to have in another pane, and a glyph alone does not say that.

**A Chip that hands something to the AI session carries a word**: `▶ AI Inject` and `◆ Skills`,
with their keys as ADR 0022 describes. The glyphs are geometric, as 0022 requires. Every other Chip
keeps glyph and keys alone.

**AI Inject sits on the panes it injects from** — the Editor and the Terminal — and the AI pane
carries Skills instead, since injecting the AI's own screen into itself is not a gesture anyone
makes.

## Consequences

When room is short the word goes with the keys, under 0022's one-shape rule: every Chip on a border
sheds together, down to glyph alone. The Nerd Font icons elsewhere in `ui::action_icon` already break
0022 and are left for their own issue rather than fixed here.
