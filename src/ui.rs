//! Drawing. Reads state, writes pixels — no decisions.

use crate::pty::Pane as PtyPane;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use varde::editor::Mode;
use varde::highlight::{self, Kind};
use varde::layout::{self, Area};
use varde::lsp;
use varde::minimap;
use varde::tree::Row;
use varde::{
    filter, keys, mark, palette_rows, reading, review, story, tools, tree, Mark, Modal, Pane,
    Place, Selection, State, View,
};

/// The bits of on-screen furniture the edge owns: what it is saying, and any
/// half-typed input it is collecting.
/// Unsaved work, wherever it is marked: the tree's row, the buffer dots and
/// the editor's own title. Bright rather than plain yellow, which sat close
/// enough to the border grey to be missed on the row you were not looking at —
/// and one name rather than three literals, because a dirty dot that means one
/// thing in the tree and another in the title is worse than no colour.
const DIRTY: Color = Color::LightYellow;

/// Orange, as the 256-colour cube spells it. Distinct from the yellow an
/// ordinary notice uses: a warning that looked like every other notice is a
/// warning nobody reads twice.
const WARNING: Color = Color::Indexed(208);
/// The gutter bar on the Utterance being spoken. Cyan is already what the
/// editor's own footer says where you are in — this says where the voice is.
const READING: Color = Color::Cyan;
/// The minimap's slider: the terminal's own foreground dimmed, since the
/// terminal owns both its palette and its background and a fixed slot has no
/// idea which side of the background it lands on.
const FAINT: Style = Style::new().fg(Color::Reset).add_modifier(Modifier::DIM);
/// How far the space dots and indentation guides stand from the page towards
/// the text, in percent. Barely there on purpose: a hint to the eye, not
/// something to read. DIM, about half way, competed with the code.
const FAINT_INK: u16 = 12;

/// The space dots and indentation guides, mixed from the terminal's own
/// foreground and background, which the edge asked it for — never a fixed
/// slot, which under Gruvbox sat darker than the page. A terminal that would
/// not say gets the minimap's DIM.
pub fn faint(palette: Option<[[u8; 3]; 2]>) -> Style {
    let Some([text, page]) = palette else {
        return FAINT;
    };
    let mix = |channel: usize| {
        ((u16::from(text[channel]) * FAINT_INK + u16::from(page[channel]) * (100 - FAINT_INK))
            / 100) as u8
    };
    Style::new().fg(Color::Rgb(mix(0), mix(1), mix(2)))
}

/// How loudly the status line says what it is saying. Replaces asking the words
/// themselves — the line used to draw the hint grey by testing it for a leading
/// space, which is a convention only its author knew and had room for exactly
/// two tones.
#[derive(Clone, Copy)]
pub enum Tone {
    /// The standing hint, shown when there is nothing else to say.
    Hint,
    /// Something happened and it went as asked.
    Notice,
    /// Something is at risk, or was refused.
    Warning,
}

pub struct Chrome<'a> {
    pub status: &'a str,
    pub tone: Tone,
    pub name_draft: &'a str,
    pub command_draft: Option<&'a str>,
    /// What is typed into the AI pane's start box while nothing is running.
    pub ai_draft: &'a str,
    /// The comment type picked so far, empty until one is chosen.
    pub comment_kind: &'a str,
    /// Typed into the tree's filter box, or None when it is not in use.
    pub filter_draft: Option<&'a str>,
    /// The current buffer's tokens, by line, parsed once per edit rather than
    /// per frame.
    pub tokens: &'a [Vec<highlight::Token>],
    /// The lines of the current buffer a Run mark stands on, found with the
    /// tokens and for their reason: a syntax tree per frame is a parse per
    /// frame.
    pub run_marks: &'a [usize],
    /// The new and old sides of the diff under review, each parsed whole when
    /// the diff was read. Whole, and both of them, because a diff interleaves
    /// two sources and neither is one when it is cut into rows — the argument
    /// is [`review::diff_tokens`]'s. Empty where a side could not be read.
    pub diff_new: &'a [Vec<highlight::Token>],
    pub diff_old: &'a [Vec<highlight::Token>],
    /// The Preview's rows, laid out at the edge and cached there against
    /// `(Buffer::revision, pane width)`. Handed in rather than derived here for
    /// the reason the tokens are: a mermaid routing pass per frame is visible,
    /// not merely wasteful.
    pub preview: &'a [varde::preview::Row],
    /// Mixed once, from the colours the terminal said it has: [`faint`].
    pub faint: Style,
}

pub struct Areas {
    pub tree: Rect,
    pub editor: Rect,
    pub ai: Rect,
    /// The strip's shells, side by side, one rectangle each (F38).
    pub splits: Vec<Rect>,
    pub band: Rect,
    pub step_menu: Rect,
    /// The corner beneath the tree. One rectangle whichever pane is in it: two
    /// would be two places a click could land, and only one of them drawn.
    pub corner: Rect,
    /// The minimap's strip, inside the editor's borders. Zero-width while the
    /// mirror is hidden — the library's answer, so the renderer and the
    /// hit-test cannot disagree about which columns are the mirror's.
    pub minimap: Rect,
    /// The layout these were drawn from, for a box over the buffer: where it
    /// lands is the library's arithmetic, which the mouse hit-tests too.
    pub panes: layout::Layout,
}

/// Rectangles for drawing, derived from the library's layout so that drawing
/// and hit-testing cannot disagree about where a pane is.
pub fn areas(area: Rect, state: &State) -> Areas {
    let panes = layout::panes(
        area.width,
        area.height,
        state.tree_divider as u16,
        state.ai_width.map(|width| width as u16),
        story::band_height(state),
        story::step_menu_width(state),
        varde::shapes(state),
    );
    Areas {
        tree: rect(panes.tree),
        editor: rect(panes.editor),
        ai: rect(panes.ai),
        splits: (0..state.terminals.len().max(1))
            .map(|k| rect(layout::split(panes.terminal, state.terminals.len(), k)))
            .collect(),
        band: rect(panes.band),
        step_menu: rect(panes.step_menu),
        corner: rect(panes.corner),
        minimap: rect(minimap::strip(state, panes.editor)),
        panes,
    }
}

fn rect(area: Area) -> Rect {
    Rect::new(area.x, area.y, area.width, area.height)
}

pub fn draw(
    frame: &mut Frame,
    state: &State,
    rows: &[Row],
    shells: &[PtyPane],
    ai: Option<&PtyPane>,
    output: Option<&PtyPane>,
    chrome: Chrome,
) {
    let areas = areas(frame.area(), state);
    draw_list_pane(frame, state, rows, &areas, &chrome);
    // Composed once: the renderer, the caret and the "is the caret free"
    // question all have to agree about whether a line is being typed.
    let typing = chrome.command_draft.map(|draft| format!(":{draft}█"));
    // The in-file search draws its own caret, and while it has the keyboard no
    // pane's caret is free either.
    let finding = state
        .find
        .as_ref()
        .is_some_and(|find| find.keys != varde::FindKeys::Away);
    // The reading surface gets a field of its own: `Black` is the theme's
    // palette 0 rather than a hex that would fight whatever palette the
    // terminal is set to, and it lifts every foreground's contrast without
    // touching their relationships. Painted on the pane's *inner* area, not
    // through `Block::style`, which covers the border ring too and reads as the
    // field spilling past the frame. `:dim` turns it off, because the same
    // opaque cell that buys the contrast is what stops a transparent window
    // showing through — which of the two is wanted is not ours to decide.
    if state.editor_field && state.editor_theme != "light" {
        frame.buffer_mut().set_style(
            areas.editor.inner(Margin::new(1, 1)),
            Style::default().bg(Color::Black),
        );
    }
    frame.render_widget(
        editor_widget(
            state,
            typing.as_deref(),
            (chrome.tokens, chrome.run_marks),
            (chrome.diff_new, chrome.diff_old),
            chrome.preview,
            areas.editor,
            chrome.faint,
        ),
        areas.editor,
    );
    if state.walking.is_some() {
        frame.render_widget(band_widget(state), areas.band);
    }
    if matches!(state.walking, Some(story::Walking::Story { .. })) {
        frame.render_widget(
            step_menu_widget(state, areas.step_menu.width),
            areas.step_menu,
        );
    }
    // Over the text, not beside it: `varde::fits` has already taken the
    // strip's columns out of the count the text is clamped against, so nothing
    // the text was allowed to reach is covered.
    minimap(frame, state, &areas, chrome.tokens);
    cheatsheet(frame, state, areas.editor);
    replace_box(frame, state, areas.panes.editor);
    if !finding {
        place_cursor(frame, state, &areas, typing.as_deref());
    }
    // One occupant at a time, exhaustively: the Strip is one rectangle, and a
    // group drawn over the one beside it is two panes claiming the same rows.
    match state.strip {
        layout::Group::Shells => {
            for (k, (shell, area)) in shells.iter().zip(&areas.splits).enumerate() {
                let title = match shells.len() {
                    1 => "terminal".to_string(),
                    _ => format!("terminal {}", k + 1),
                };
                let focused = state.focus == Pane::Terminal && k == state.split();
                frame.render_widget(
                    terminal_widget(shell, &title, focused, state, Pane::Terminal),
                    *area,
                );
            }
        }
        layout::Group::Debug => {
            frame.render_widget(
                variables_widget(state, areas.panes.terminal.width, chrome.name_draft),
                rect(areas.panes.terminal),
            );
            // Zero-width while it is hidden, so there is nothing to draw and
            // the Variables already have the columns back.
            if let (Some(output), true) = (output, areas.panes.output.width > 0) {
                frame.render_widget(
                    terminal_widget(
                        output,
                        "program",
                        state.focus == Pane::Output,
                        state,
                        Pane::Output,
                    ),
                    rect(areas.panes.output),
                );
            }
        }
    }
    strip_chips(frame, state, areas.panes.strip());
    group_tabs(frame, state, areas.panes.strip());
    // Zero-width while the corner is empty, so there is nothing to draw and
    // nothing to clear: the shell already has the columns back. Exhaustive on
    // the occupant, so a new pane in that slot is a compiler error rather than
    // a corner that quietly draws nothing.
    if areas.corner.width > 0 {
        match state.corner {
            layout::Corner::Hidden => {}
            layout::Corner::Risk => {
                frame.render_widget(risk_widget(state, areas.corner.width), areas.corner)
            }
            layout::Corner::Buffers => {
                frame.render_widget(buffers_widget(state, areas.corner.width), areas.corner)
            }
            layout::Corner::History => {
                frame.render_widget(history_widget(state, areas.corner.width), areas.corner)
            }
            layout::Corner::Breakpoints => {
                frame.render_widget(breakpoints_widget(state, areas.corner.width), areas.corner)
            }
            layout::Corner::Frames => {
                frame.render_widget(frames_widget(state, areas.corner.width), areas.corner)
            }
            layout::Corner::Diagnostics(showing) => {
                frame.render_widget(diagnostics_widget(state, areas.corner.width), areas.corner);
                severity_labels(frame, state, showing, areas.panes.corner);
            }
            layout::Corner::Conflicts => {
                frame.render_widget(conflicts_widget(state, areas.corner.width), areas.corner)
            }
        }
    }
    let caret_is_free = state.modal == Modal::None && typing.is_none() && !finding;
    if let (Pane::Output, layout::Group::Debug, true, Some(output)) =
        (state.focus, state.strip, caret_is_free, output)
    {
        place_pty_cursor(frame, rect(areas.panes.output), output);
    }
    if let (Pane::Terminal, layout::Group::Shells, true, Some((shell, area))) = (
        state.focus,
        state.strip,
        caret_is_free,
        shells.iter().zip(&areas.splits).nth(state.split()),
    ) {
        place_pty_cursor(frame, *area, shell);
    }
    draw_ai_pane(frame, state, &areas, ai, &chrome, caret_is_free);
    draw_status(frame, state, &chrome);
    // On the editor's text, under everything that floats over it.
    if let Some((x, y)) = varde::debug::edit_chip(state, &areas.panes) {
        frame.render_widget(
            Paragraph::new(action_icon(varde::debug::EDIT)),
            Rect::new(x, y, 1, 1),
        );
    }

    // Over the panes rather than under them: the box is wrapped to the screen,
    // so it overhangs the editor's own rectangle, and anything drawn after it
    // paints over it — which is how its right-hand half came to sit behind the
    // AI pane's border. A modal still outranks it, below.
    hover(frame, state, &areas.panes);
    breakpoint_reason(frame, state, &areas.panes);
    diagnostic_box(frame, state, &areas.panes);
    candidates(frame, state, &areas.panes);
    // Over the panes for the Hover's reason, and after it: the window is the
    // thing the reader is working in, so nothing floats above it but a modal.
    evaluator(frame, state, &areas.panes);

    // Search floats over the panes rather than replacing them: you can still
    // see where you were.
    if state.search.is_some() {
        search_screen(frame, state);
        return;
    }
    draw_modal(frame, state, &chrome);
}

/// The pane the tree shares with Review's changed-files list and Story's
/// spine. `t` toggles the last two; the changed-files list is the very one
/// Review view shows, joined here rather than replaced.
fn draw_list_pane(frame: &mut Frame, state: &State, rows: &[Row], areas: &Areas, chrome: &Chrome) {
    match state.view {
        View::Edit => {
            frame.render_widget(tree_widget(state, rows, areas.tree.width), areas.tree);
            filter_box(frame, state, areas.tree, chrome.filter_draft);
        }
        View::Review => frame.render_widget(review_widget(state, areas.tree.width), areas.tree),
        View::Story => match state.story_listing {
            story::Listing::Spine => frame.render_widget(spine_widget(state), areas.tree),
            story::Listing::Files => {
                frame.render_widget(review_widget(state, areas.tree.width), areas.tree)
            }
        },
    }
}

/// The AI pane, or the box asking which CLI to start in it.
fn draw_ai_pane(
    frame: &mut Frame,
    state: &State,
    areas: &Areas,
    ai: Option<&PtyPane>,
    chrome: &Chrome,
    caret_is_free: bool,
) {
    let Some(pane) = ai else {
        frame.render_widget(start_ai_widget(state, chrome.ai_draft), areas.ai);
        if state.focus == Pane::Ai && state.modal == Modal::None {
            let inner = areas.ai.width.saturating_sub(2);
            let width = inner.min(24);
            frame.set_cursor_position((
                areas.ai.x + 1 + (inner - width) / 2 + 1 + chrome.ai_draft.chars().count() as u16,
                areas.ai.y + areas.ai.height / 2,
            ));
        }
        return;
    };
    frame.render_widget(
        terminal_widget(pane, "ai", state.focus == Pane::Ai, state, Pane::Ai),
        areas.ai,
    );
    if state.focus == Pane::Ai && caret_is_free {
        place_pty_cursor(frame, areas.ai, pane);
    }
}

fn draw_status(frame: &mut Frame, state: &State, chrome: &Chrome) {
    let row = Rect {
        y: frame.area().height.saturating_sub(1),
        height: 1,
        ..frame.area()
    };
    // The whole row is cleared but only its middle is written to, so the status
    // sits inside the columns the panes' borders occupy rather than on them.
    let line = row.inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    frame.render_widget(Clear, row);
    frame.render_widget(
        Paragraph::new(status_line(
            chrome.status,
            chrome.tone,
            &state.running_version,
            state.update.as_deref(),
            line.width,
        )),
        line,
    );
}

/// The bottom row `width` columns wide: the status on the left, and the
/// version tag hard against the right edge in every view, so it is never a
/// notice the next notice replaces. The status is cut short before the tag.
/// Where the row is too narrow for both, the tag gives up its hint and then
/// itself — a tag may take at most half the row, since the notice is what
/// the user is being told right now.
fn status_line(
    status: &str,
    tone: Tone,
    running: &str,
    update: Option<&str>,
    width: u16,
) -> Line<'static> {
    let width = width as usize;
    let version = |text: &str| {
        format!(
            "v{}",
            text.chars().filter(|c| !c.is_control()).collect::<String>()
        )
    };
    let (forms, colour) = match update {
        None => (vec![version(running)], Color::Green),
        Some(newer) => {
            let short = format!("{} → {}", version(running), version(newer));
            (
                vec![format!("{short}  C-space u to update"), short],
                Color::Blue,
            )
        }
    };
    let tone = Style::default().fg(match tone {
        Tone::Hint => Color::DarkGray,
        Tone::Notice => Color::Yellow,
        Tone::Warning => WARNING,
    });
    let Some(tag) = forms.into_iter().find(|tag| 2 * (tag.width() + 1) <= width) else {
        return Line::from(Span::styled(truncate(status, width), tone));
    };
    let room = width - tag.width() - 1;
    let status = truncate(status, room);
    let gap = " ".repeat(room - status.width() + 1);
    Line::from(vec![
        Span::styled(status, tone),
        Span::raw(gap),
        Span::styled(tag, Style::default().fg(colour)),
    ])
}

/// The words live here: the core only says which question is being asked,
/// which range is confirmed and that the buffer diverged, so no scenario
/// asserts on wording.
fn draw_modal(frame: &mut Frame, state: &State, chrome: &Chrome) {
    match &state.modal {
        // The screen's height goes in because the row count depends on it —
        // the same number the mouse hit-test passes.
        Modal::Palette => overlay(
            frame,
            "COMMANDS",
            rows_lines(palette_rows(frame.area().height)),
        ),
        Modal::Chord => overlay(frame, "SPACE", rows_lines(keys::chord_rows(state))),
        Modal::Breakpoint { field, draft, .. } => {
            overlay(frame, "BREAKPOINT", breakpoint_box_lines(*field, draft))
        }
        Modal::NameBox { .. } => overlay(
            frame,
            "NAME",
            vec![Line::from(chrome.name_draft.to_string())],
        ),
        Modal::ExceptionClass => overlay(
            frame,
            "PAUSE ON EXCEPTION CLASS",
            vec![Line::from(chrome.name_draft.to_string())],
        ),
        // Drawn by `variables_lines`, in the row being acted on: an overlay
        // would cover the very row whose value is being written.
        Modal::SetValue | Modal::NewWatch => {}
        Modal::Comment => overlay(frame, "COMMENT", comment_lines(state, chrome)),
        Modal::ConfirmSubmit => overlay(
            frame,
            "SUBMIT",
            vec![
                Line::from("  Sending the review clears the AI's prompt line."),
                Line::from("  Anything half-written there is lost."),
                Line::from(""),
                Line::from("  (y) send    (n) cancel"),
            ],
        ),
        Modal::ConfirmStory { spelling, out: _ } => overlay(
            frame,
            "STORY",
            vec![
                Line::from(format!("  Author a story for \"{spelling}\"?")),
                Line::from("  This clears the AI's prompt line."),
                Line::from(""),
                Line::from("  (y) author    (n) cancel"),
            ],
        ),
        Modal::Tools { row } => {
            let height = frame.area().height;
            overlay(frame, "TOOLS", tool_lines(state, *row, height))
        }
        Modal::Launches { row } => {
            let height = frame.area().height;
            overlay(frame, "LAUNCH", launch_lines(state, *row, height))
        }
        Modal::Branches { refs, filter, row } => {
            let height = frame.area().height;
            overlay(
                frame,
                "BRANCHES",
                branch_lines(&story::branches(refs, filter), filter, *row, height),
            )
        }
        // The one thing a restart answers, said out loud: a shell profile is
        // not this process's environment, and Varde cannot reach one from
        // inside itself. So the box says what will happen — Varde leaves, and
        // starting it again is the reader's — rather than promising to come
        // back (R31.24, Q59).
        Modal::Restart => overlay(
            frame,
            "RESTART",
            vec![
                Line::from("  The command is still not on this process's PATH."),
                Line::from("  An installer that edited a shell profile cannot be"),
                Line::from("  seen from here — Varde inherited its PATH at launch."),
                Line::from(""),
                Line::from("  (y) quit, so you can start Varde again    (n) stay"),
            ],
        ),
        Modal::RunMark { .. } => run_offer(frame, state),
        Modal::Diverged => overlay(frame, "DIVERGED", diverged_lines(state)),
        Modal::StepDetail => overlay(frame, "STEP", step_detail_lines(state)),
        Modal::Prediction { .. } => overlay(frame, "PREDICTION", prediction_lines(state)),
        // Drawn by `candidates` instead, beside the hover box it resembles: it
        // is placed against the line being typed rather than centred over
        // everything, which is the whole of what "does not cover the line
        // being edited" means.
        Modal::Candidates(_) => {}
        // A sequence of tab stops is a condition the cursor is in, not
        // something to put on screen: what it has to say is *where the cursor
        // is*, and the cursor is already drawn there.
        Modal::Stops { .. } => {}
        Modal::None => {}
    }
}

fn diverged_lines(state: &State) -> Vec<Line<'static>> {
    vec![
        Line::from(format!(
            "  {} changed on disk while you had unsaved edits.",
            state
                .current_buffer
                .as_ref()
                .and_then(|path| path.file_name())
                .unwrap_or_default()
                .to_string_lossy()
        )),
        Line::from(""),
        Line::from("  (r) reload from disk — your edits are lost"),
        Line::from("  (w) overwrite disk — the change on disk is lost"),
        Line::from("  (m) ask the AI to merge both"),
        Line::from(""),
        Line::from("  (Esc) decide later"),
    ]
}

/// The Step's detail overlay: why it exists, what flows in and out, and the
/// Nudge — omitted entirely when the Step has none, since a section with
/// nothing to say is not "empty", it is a promise the overlay does not make.
/// The claim is repeated at the top: the band under the code is gone once
/// this overlay is centred over the screen.
fn step_detail_lines(state: &State) -> Vec<Line<'static>> {
    let Some(step) = story::current_step(state) else {
        return vec![Line::from("")];
    };
    let mut lines = vec![
        Line::from(format!("  {}", step.claim)),
        Line::from(""),
        Line::from(Span::styled(
            "  Why",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("  {}", step.why)),
    ];
    match story::staleness(state, step) {
        story::Staleness::Fresh => {}
        stale => {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Stale",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from("  stored:"));
            lines.extend(indented_lines(&step.site.text));
            match stale {
                story::Staleness::Gone => lines.push(Line::from("  now: the file is gone")),
                story::Staleness::TooShort => {
                    lines.push(Line::from("  now: the file no longer reaches this range"))
                }
                story::Staleness::Changed { now } => {
                    lines.push(Line::from("  now:"));
                    lines.extend(indented_lines(&now));
                }
                story::Staleness::Fresh => unreachable!("matched above"),
            }
        }
    }
    if let Some(flow) = &step.flow {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Flow",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(format!("  in:  {}", flow.flow_in)));
        lines.push(Line::from(format!("  out: {}", flow.flow_out)));
    }
    if let Some(nudge) = &step.nudge {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Nudge",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(format!("  {nudge}")));
    }
    lines
}

/// The prediction overlay: the question, then either every choice — numbered,
/// so a keypress maps to one directly — or, once a correct pick has replaced
/// them, that choice's feedback alone. A wrong pick's feedback is shown above
/// the choices, which stay up so the reviewer can try again.
fn prediction_lines(state: &State) -> Vec<Line<'static>> {
    let Some(prediction) = story::current_step(state).and_then(|step| step.prediction.as_ref())
    else {
        return vec![Line::from("")];
    };
    let mut lines = vec![
        Line::from(format!("  {}", prediction.question)),
        Line::from(""),
    ];
    if let Some(feedback) = story::prediction_feedback(state) {
        lines.push(Line::from(format!("  {feedback}")));
        lines.push(Line::from(""));
    }
    for (index, text) in story::prediction_choices(state).into_iter().enumerate() {
        lines.push(Line::from(format!("  ({}) {}", index + 1, text)));
    }
    lines
}

/// A Site's text, one `Line` per line it holds — never one `Line` carrying an
/// embedded `\n`, which `ratatui` renders as a control character rather than
/// a break.
fn indented_lines(text: &str) -> Vec<Line<'static>> {
    text.split('\n')
        .map(|line| Line::from(format!("    {line}")))
        .collect()
}

fn pane_block(title: impl Into<Line<'static>>, state: &State, pane: Pane) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(title.into())
        .border_style(Style::default().fg(border_colour(state, pane)))
}

fn border_colour(state: &State, pane: Pane) -> Color {
    if state.focus == pane {
        Color::Cyan
    } else {
        Color::DarkGray
    }
}

/// `tree::title`, and after it the project's error and warning totals in the
/// colours the gutter marks them in — the strings `mouse::nudge_at` measures.
fn tree_title(state: &State) -> Line<'static> {
    let mut spans = vec![Span::raw(varde::tree::title(state))];
    for (severity, label) in varde::lsp::nudge(state) {
        spans.push(Span::styled(
            label,
            Style::default().fg(severity_colour(severity)),
        ));
    }
    Line::from(spans)
}

/// The Risk pane's name and what the figure is: `measuring` while nothing has
/// been measured yet, `stale` over a list that describes code the workspace has
/// moved past, and how much of the workspace the figure does not describe. On
/// the border for the reason the tree's figure is — the pane is narrow, and
/// rows inside the borders are for rows.
fn risk_title(state: &State, width: u16) -> String {
    use varde::risk::Standing;
    // What is left of the border after its two corners and the action icons at
    // the far end. Truncated to it rather than left to overrun: ratatui draws a
    // left-aligned title over a right-aligned one, and a title long enough to
    // reach the icons would leave a stop that can be clicked and not seen.
    let room = (width as usize).saturating_sub(2 + 2 * varde::risk::pane_actions(state).len());
    let mut title = "risk".to_string();
    // While a loop is running the border is the loop's. The pane is thirty
    // columns wide, so it cannot say both; the figure is in the rows below it
    // and on the tree's border either way, and an unlabelled wait is
    // indistinguishable from a hang.
    if state.refactor.running.is_some() {
        if let Some(status) = varde::risk::status(state) {
            title.push_str("  ");
            title.push_str(&status);
        }
        return truncate(&title, room);
    }
    // The selected row's file. It is on the border and not in the row because
    // the pane is narrower than a path — which is what buys one ordering rule,
    // worst first across the whole Scope, instead of rows under file headings.
    if let Some(function) = varde::risk::selected(state) {
        title.push_str("  ");
        title.push_str(&function.file);
    }
    // Exhaustive, so a fifth thing the figure can be is a compiler error here
    // rather than a border that quietly says nothing about it.
    let said = match varde::risk::standing(state) {
        Standing::Computing => Some("measuring"),
        Standing::Stale => Some("stale"),
        Standing::NothingAnalysed => Some("nothing measured"),
        Standing::Computed => None,
    };
    if let Some(said) = said {
        title.push_str("  ");
        title.push_str(said);
    }
    let unparsed = varde::risk::unparsed(state);
    if unparsed > 0 {
        title.push_str(&format!("  ·  {unparsed} unparsed"));
    }
    // Once a loop is over the condition that stopped it *trails* what the pane
    // normally says rather than replacing it: `Refactor::stopped` is only
    // cleared by the next start, and a border that hijacked itself for the rest
    // of the session would never name the selected row's file again.
    if let Some(status) = varde::risk::status(state) {
        title.push_str("  ·  ");
        title.push_str(&status);
    }
    truncate(&title, room)
}

/// The worklist: a Function's name and its figure, worst first. The path is not
/// here — the pane is narrower than one, and the selected row's file goes in
/// the border instead.
fn risk_widget(state: &State, width: u16) -> Paragraph<'static> {
    Paragraph::new(risk_lines(state, width))
        .scroll((state.risk_scroll as u16, 0))
        .block(
            pane_block(risk_title(state, width), state, Pane::Risk)
                .title(pane_actions_title(state)),
        )
}

/// The pane's own action icons, on the right-hand end of its top border: the
/// recompute, and the loop's start or its stop in the one slot. Two columns
/// each, hard against the corner — `mouse::pane_action_at` hit-tests exactly
/// those columns, and a test below pins ratatui's placement of a right-aligned
/// title so the two cannot drift.
fn pane_actions_title(state: &State) -> Line<'static> {
    // Lit here too, and not only in the rows: the arrows step past the last row
    // onto these icons (`risk::on_actions`), and an armed action drawn the same
    // grey as the two beside it is a keyboard position nothing on screen shows.
    let on_them = varde::risk::on_actions(state);
    Line::from(
        varde::risk::pane_actions(state)
            .into_iter()
            .enumerate()
            .flat_map(|(at, action)| {
                let armed = on_them && state.selected_action == Some(at);
                [
                    Span::styled(action_icon(action), action_style(state, action, armed)),
                    Span::raw(" "),
                ]
            })
            .collect::<Vec<Span<'static>>>(),
    )
    .right_aligned()
}

/// An action icon's three states: available actions stay quiet, the one Enter
/// would run is lit, and the one the pointer is resting on is lit too.
///
/// The pointer's state is the same colour as the keyboard's on purpose — both
/// say "this is what pressing does something to", and a terminal has no hand
/// pointer to say it any other way. Shared once the pane's own icons became
/// the third list to draw them: a position shown one way in the tree, another
/// in the worklist and a third on the border is three chances to look like
/// nothing is selected.
fn action_style(state: &State, action: &str, armed: bool) -> Style {
    match armed || state.hovered_action == Some(action) {
        true => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        false => Style::default().fg(Color::DarkGray),
    }
}

/// One row per open buffer: the mark the editor's dot strip draws, then the
/// file's name. The selected row's whole path goes in the border for the reason
/// the Risk list's does — the pane is the tree's width and a path does not fit
/// a row — and the pane draws no action icons at all, because it has none: its
/// rows name a buffer to switch to, and switching is the whole of what they do.
fn buffers_widget(state: &State, width: u16) -> Paragraph<'static> {
    let mut title = "buffers".to_string();
    if let Some(path) = varde::buffer_selected(state) {
        title.push_str("  ");
        title.push_str(&varde::relative(state, path));
    }
    Paragraph::new(buffers_lines(state, width))
        .scroll((state.buffers_scroll as u16, 0))
        .block(pane_block(
            truncate(&title, width.saturating_sub(2) as usize),
            state,
            Pane::Buffers,
        ))
}

/// Split out of `buffers_widget` for the reason `risk_lines` is split out of
/// `risk_widget`: a `Paragraph` will not give its text back, so a test cannot
/// see what was drawn.
fn buffers_lines(state: &State, width: u16) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(2) as usize;
    varde::buffer_list(state)
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            let selected = match index == state.buffers_selection {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
            // The file's name, not its path: the path is on the border.
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| varde::relative(state, path));
            // The same `Mark` and the same `glyph` the dot strip draws: two
            // views of one fact, so a buffer cannot read as unsaved on the
            // editor's bottom edge and merely open here.
            let (mark, colour) = glyph(varde::mark(state, path));
            let room = inner.saturating_sub(2);
            Line::from(vec![
                Span::styled(mark, selected.fg(colour)),
                Span::styled(format!(" {}", truncate(&name, room)), selected),
            ])
        })
        .collect()
}

/// One row per Breakpoint, its Transport on the top border at the columns
/// `mouse::breakpoint_chip_at` hit-tests.
fn breakpoints_widget(state: &State, width: u16) -> Paragraph<'static> {
    let chips = varde::debug::transport(state);
    let labels = layout::chip_labels(&chips, width, layout::CORNER_TITLE);
    let gap = Style::default().fg(border_colour(state, Pane::Breakpoints));
    let mut spans = Vec::new();
    for (chip, label) in chips.iter().zip(labels) {
        spans.extend(chip_spans(state, chip, label));
        spans.push(Span::styled("\u{2500}", gap));
    }
    Paragraph::new(breakpoints_lines(state, width))
        .scroll((state.breakpoints_scroll as u16, 0))
        .block(
            pane_block("breakpoints", state, Pane::Breakpoints)
                .title(Line::from(spans).right_aligned()),
        )
}

/// Split out of `breakpoints_widget` for the reason `risk_lines` is split out
/// of `risk_widget`: a `Paragraph` will not give its text back.
///
/// The line and the icon are taken out of the row's columns first, so a narrow
/// pane cuts the path and never slides the icon off the columns `mouse`
/// hit-tests it from. A Stale one says so, dimmed.
fn breakpoints_lines(state: &State, width: u16) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(2) as usize;
    let switches = varde::debug::switches(state);
    let above = switches.len();
    let mut lines: Vec<Line<'static>> = switches
        .into_iter()
        .enumerate()
        .map(|(index, (filter, on))| {
            let style = match index == state.breakpoints_selection {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
            let glyph = match on {
                true => "\u{25a0}",
                false => "\u{25a1}",
            };
            Line::from(Span::styled(
                truncate(&format!(" {glyph} {}", filter.label), inner),
                style,
            ))
        })
        .collect();
    lines.extend(
        varde::debug::list(state)
            .into_iter()
            .enumerate()
            .map(|(index, breakpoint)| {
                let index = index + above;
                let on_this_row = index == state.breakpoints_selection;
                let selected = match on_this_row {
                    true => Style::default().add_modifier(Modifier::REVERSED),
                    false => Style::default(),
                };
                let actions = match on_this_row {
                    true => varde::debug::row_actions(state),
                    false => Vec::new(),
                };
                let tail = match breakpoint.stale {
                    true => format!(":{} stale ", breakpoint.line),
                    false => format!(":{} ", breakpoint.line),
                };
                let room = inner.saturating_sub(tail.width() + 1 + actions.len() * 2);
                let path = match room {
                    0 => String::new(),
                    room => truncate(&varde::relative(state, &breakpoint.file), room),
                };
                let mut spans = vec![Span::styled(format!(" {path:<room$}"), selected)];
                spans.push(Span::styled(tail, selected.fg(Color::DarkGray)));
                for (at, action) in actions.iter().enumerate() {
                    spans.push(Span::styled(
                        action_icon(action),
                        action_style(state, action, state.selected_action == Some(at)),
                    ));
                    spans.push(Span::raw(" "));
                }
                Line::from(spans)
            }),
    );
    lines
}

/// The Frames grouped by thread, the inspected call marked.
fn frames_widget(state: &State, width: u16) -> Paragraph<'static> {
    let widget = Paragraph::new(frames_lines(state, width))
        .scroll((state.frames_scroll as u16, 0))
        .block(pane_block("frames", state, Pane::Frames));
    dimmed_while_running(state, widget)
}

/// What the last pause left on screen is drawn dimmed while the program runs,
/// so nothing stale is mistaken for current. One answer for the two panes that
/// show it, because it is one fact about the session rather than two about the
/// panes — the scenarios assert that fact, and which panes read it is edge
/// work, verified by running it.
fn dimmed_while_running(state: &State, widget: Paragraph<'static>) -> Paragraph<'static> {
    match varde::debug::stale(state) {
        true => widget.style(Style::default().add_modifier(Modifier::DIM)),
        false => widget,
    }
}

/// Split out of `frames_widget` for the reason `risk_lines` is: a `Paragraph`
/// will not give its text back. A thread heads its calls, flagged when it is
/// held Paused elsewhere; a call's name comes first and its place after, the
/// name cut before the place is: which call it is reads off the name, and the
/// file and line are the Paused line the row would move to. A folded run of
/// Library frames is one dimmed row that says how many.
fn frames_lines(state: &State, width: u16) -> Vec<Line<'static>> {
    use varde::debug::FrameRow;
    let inner = width.saturating_sub(2) as usize;
    let chosen = varde::debug::paused_line(state);
    let frames = varde::debug::frames(state);
    varde::debug::frame_rows(state)
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let style = match index == state.frames_selection {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
            let frame = match row {
                FrameRow::Thread {
                    name,
                    paused,
                    child,
                    ..
                } => {
                    let flag = match paused {
                        true => " \u{2016}",
                        false => "",
                    };
                    let name = match child {
                        Some(child) => format!("{child} \u{203a} {name}"),
                        None => name.clone(),
                    };
                    let room = inner.saturating_sub(flag.width() + 1);
                    return Line::from(vec![
                        Span::styled(
                            format!(" {:<room$}", truncate(&name, room)),
                            style.add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(flag, style.fg(Color::Yellow)),
                    ]);
                }
                FrameRow::Library { count, .. } => {
                    let text = format!("   \u{22ef} {count} library frames");
                    return Line::from(Span::styled(
                        format!("{:<inner$}", truncate(&text, inner)),
                        style.add_modifier(Modifier::DIM),
                    ));
                }
                FrameRow::Frame(index) => &frames[*index],
            };
            let mut style = style;
            let place = match &frame.file {
                Some(file) => format!(
                    " {}:{} ",
                    file.file_name().unwrap_or_default().to_string_lossy(),
                    frame.line
                ),
                None => " ".to_string(),
            };
            if chosen.is_some_and(|(file, line, _)| {
                frame.file.as_deref() == Some(file) && frame.line == line
            }) {
                style = style.add_modifier(Modifier::BOLD);
            }
            let room = inner.saturating_sub(place.width() + 3);
            let name = truncate(&frame.name, room);
            Line::from(vec![
                Span::styled(format!("   {name:<room$}"), style),
                Span::styled(place, style.fg(Color::DarkGray)),
            ])
        })
        .collect()
}

/// The Variables of the chosen Frame, in the Strip's Debug group. Its title
/// says which of the two it is drawing: this pause, or the last one.
fn variables_widget(state: &State, width: u16, draft: &str) -> Paragraph<'static> {
    let widget = Paragraph::new(variables_lines(state, width, draft))
        .scroll((state.variables_scroll as u16, 0))
        .block(pane_block(
            varde::debug::title(state),
            state,
            Pane::Variables,
        ));
    dimmed_while_running(state, widget)
}

/// Split out of `variables_widget` for the reason `frames_lines` is. One row
/// per member the tree has open: its depth as indentation, whether it opens,
/// its name, what the adapter's presentation hint says about it, and its
/// value.
fn variables_lines(state: &State, width: u16, draft: &str) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(2) as usize;
    varde::debug::variables(state)
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let style = match index == state.variables_selection {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
            // The box is the row: what is being typed stands where the value
            // or the new Watch will, so the eye never leaves the row it is
            // acting on.
            if index == state.variables_selection {
                if let Some(box_name) = match state.modal {
                    Modal::SetValue => Some(row.name.as_str()),
                    Modal::NewWatch => Some("watch"),
                    _ => None,
                } {
                    return Line::from(vec![
                        Span::styled(format!(" {box_name} "), style),
                        Span::styled(
                            truncate(draft, inner.saturating_sub(box_name.width() + 2)),
                            Style::default().add_modifier(Modifier::UNDERLINED),
                        ),
                    ]);
                }
            }
            let marker = match (row.opens, row.open) {
                (varde::debug::Opens::Nothing, _) => "  ",
                (_, true) => "\u{25be} ",
                (_, false) => "\u{25b8} ",
            };
            let hint = match row.hint {
                varde::debug::Hint::Plain => String::new(),
                hint => format!(" {}", hint.as_str()),
            };
            let mark = match row.of {
                // A Watch that calls something runs that call again at every
                // pause, which is the one thing about a Watch a reader has to
                // be told before they add it.
                varde::debug::Of::Watch { calling: true, .. } => " calls",
                _ => "",
            };
            let name = format!(
                " {}{marker}{}{hint}{mark}",
                " ".repeat(row.depth * 2),
                row.name
            );
            let chips = varde::debug::row_chips(state, index);
            let room = inner
                .saturating_sub(name.width().min(inner))
                .saturating_sub(chips.len() * 2);
            let mut spans = vec![
                Span::styled(truncate(&name, inner), style),
                // Padded to the columns left over, so the Chips sit hard
                // against the right-hand border — the very columns
                // `mouse::icon_at` hit-tests them from.
                Span::styled(
                    format!(
                        " {:<pad$}",
                        truncate(&row.value, room.saturating_sub(1)),
                        pad = room.saturating_sub(1)
                    ),
                    style.fg(match row.of {
                        varde::debug::Of::Watch { failed: true, .. } => WARNING,
                        _ => Color::DarkGray,
                    }),
                ),
            ];
            for (at, chip) in chips.iter().enumerate() {
                spans.push(Span::styled(
                    chip.glyph.clone(),
                    chip_style(state, chip, state.selected_action == Some(at)),
                ));
                spans.push(Span::raw(" "));
            }
            Line::from(spans)
        })
        .collect()
}

/// The Diagnostic list: its rows, and its name on the border while the
/// Severity labels leave room for it.
fn diagnostics_widget(state: &State, width: u16) -> Paragraph<'static> {
    let labels = varde::lsp::severity_labels(state, width);
    let title = match layout::strip_width(&labels) + layout::CORNER_TITLE <= width {
        true => "diagnostics",
        false => "",
    };
    Paragraph::new(diagnostics_lines(state, width))
        .scroll((state.diagnostics_scroll as u16, 0))
        .block(pane_block(title, state, Pane::Diagnostics))
}

/// A heading per file and a row per Diagnostic under it, cut to the pane with
/// `…`. Split out of `diagnostics_widget` for the reason `risk_lines` is split
/// out of `risk_widget`.
fn diagnostics_lines(state: &State, width: u16) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(2) as usize;
    varde::lsp::listed(state)
        .into_iter()
        .enumerate()
        .map(|(index, (path, diagnostic))| {
            let style = match index == state.diagnostics_selection {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
            match diagnostic {
                // A path is a filename somebody chose, so it is untrusted too.
                None => Line::from(Span::styled(
                    truncate(
                        &format!(" {}", varde::relative(state, path))
                            .replace(|character: char| character.is_control(), ""),
                        inner,
                    ),
                    style.add_modifier(Modifier::BOLD),
                )),
                Some(diagnostic) => Line::from(Span::styled(
                    truncate(&format!("   {}", varde::lsp::row_text(diagnostic)), inner),
                    style,
                )),
            }
        })
        .collect()
}

fn conflicts_widget(state: &State, width: u16) -> Paragraph<'static> {
    Paragraph::new(conflicts_lines(state, width))
        .scroll((state.conflicts_scroll as u16, 0))
        .block(pane_block(
            varde::conflict::title(state),
            state,
            Pane::Conflicts,
        ))
}

/// A row per unmerged file and one per Conflict under it, cut to the pane.
fn conflicts_lines(state: &State, width: u16) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(2) as usize;
    varde::conflict::listed(state)
        .into_iter()
        .enumerate()
        .map(|(index, (path, conflict))| {
            let style = match (index == state.conflicts_selection, conflict) {
                (true, _) => Style::default().add_modifier(Modifier::REVERSED),
                (false, None) => Style::default().add_modifier(Modifier::BOLD),
                (false, Some(_)) => Style::default(),
            };
            let text = varde::conflict::row_text(state, &path, conflict);
            Line::from(Span::styled(truncate(&format!(" {text}"), inner), style))
        })
        .collect()
}

/// The Severity labels over the Diagnostic list's top border, right-aligned at
/// the columns `mouse` hit-tests them from with `layout::strip_at`, the one
/// showing lit — drawn the way the Strip's Group tabs are.
fn severity_labels(frame: &mut Frame, state: &State, showing: lsp::Severity, corner: Area) {
    let labels = varde::lsp::severity_labels(state, corner.width);
    let Some(mut x) = corner
        .right()
        .saturating_sub(1)
        .checked_sub(layout::strip_width(&labels))
    else {
        return;
    };
    for (severity, label) in lsp::Severity::ALL.into_iter().zip(labels) {
        let style = match severity == showing {
            true => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::REVERSED),
            false => Style::default().fg(Color::DarkGray),
        };
        let columns = label.width() as u16;
        frame.render_widget(
            Span::styled(label, style),
            Rect::new(x, corner.y, columns, 1),
        );
        x += columns + 1;
    }
}

/// One row per Visit: which file it was in, what the cursor was standing on,
/// and the line. The selected row's whole relative path goes in the border for
/// the reason the two panes beside it put theirs there — the pane is the tree's
/// width and a path does not fit a row.
fn history_widget(state: &State, width: u16) -> Paragraph<'static> {
    Paragraph::new(history_lines(state, width))
        .scroll((state.history_scroll as u16, 0))
        .block(pane_block(
            history_title(state, width),
            state,
            Pane::History,
        ))
}

/// The pane's name, and the whole relative path of the row the keyboard is on.
/// Split out for the reason `risk_title` is: a title built inside the widget is
/// a title no test can read, and this one is the pane's only answer to *which
/// file* a row is in — the row itself carries the file's name alone, the pane
/// being narrower than a path. Truncated rather than left to overrun, as
/// `risk_title` is and for the same reason.
fn history_title(state: &State, width: u16) -> String {
    let mut title = "history".to_string();
    if let Some(visit) = varde::history::selected(state) {
        title.push_str("  ");
        title.push_str(&visit.file);
    }
    truncate(&title, width.saturating_sub(2) as usize)
}

/// Split out of `history_widget` for the reason `risk_lines` is split out of
/// `risk_widget`: a `Paragraph` will not give its text back.
///
/// The excerpt is coloured exactly as the editor colours that file. Highlighted
/// out of context, so a row cut from inside a block comment or a multi-line
/// string reads as code — `highlight::highlight`'s own doc says why, and it is
/// an accepted cost for one line in a list you are scanning rather than
/// reading. A reader will file that as a bug; it is the same trade the search
/// results box already makes.
fn history_lines(state: &State, width: u16) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(2) as usize;
    let dark = state.editor_theme != "light";
    varde::history::list(state)
        .iter()
        .enumerate()
        .map(|(index, visit)| {
            let on_this_row = index == state.history_selection;
            let selected = match on_this_row {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
            let actions = match on_this_row {
                true => varde::history::row_actions(state),
                false => Vec::new(),
            };
            // The file's name, not its path: the pane is thirty columns wide.
            let name = std::path::Path::new(&visit.file)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| visit.file.clone());
            // The line and the icon are what must survive a narrow pane, so
            // they are taken out of the row's columns first and the name and
            // the excerpt share what is left. Nothing is drawn at all with no
            // columns for it: `truncate` gives back an ellipsis a column wide
            // even for no room, and one column over the inner width slides the
            // icon off the columns `mouse::history_action_at` hit-tests it from.
            let at_line = format!(" {}", visit.line);
            let room = inner.saturating_sub(at_line.chars().count() + actions.len() * 2);
            let head = match room {
                0 => String::new(),
                room => truncate(&format!(" {name}: "), room),
            };
            let left = room.saturating_sub(head.chars().count());
            let excerpt = match left {
                0 => String::new(),
                left => truncate(
                    &varde::history::excerpt(&visit.text, visit.column, varde::history::WORDS),
                    left,
                ),
            };
            let mut spans = Vec::new();
            if !head.is_empty() {
                spans.push(Span::styled(head, selected.fg(Color::Cyan)));
            }
            if !excerpt.is_empty() {
                // A row whose line has moved on still says what it recorded,
                // and stops claiming to be current: dimmed rather than
                // syntax-coloured, which is what `CONTEXT.md`'s Stale does to a
                // Risk figure. Only a file still open can be asked — nothing
                // here may read the disk.
                match varde::history::stale(state, visit) {
                    true => spans.push(Span::styled(excerpt.clone(), selected.fg(Color::DarkGray))),
                    false => spans.extend(coloured(&visit.file, &excerpt, dark, on_this_row)),
                }
            }
            spans.push(Span::styled(
                " ".repeat(left.saturating_sub(excerpt.chars().count())),
                selected,
            ));
            spans.push(Span::styled(at_line, selected.fg(Color::DarkGray)));
            for (at, action) in actions.iter().enumerate() {
                spans.push(Span::styled(
                    action_icon(action),
                    action_style(state, action, state.selected_action == Some(at)),
                ));
                spans.push(Span::raw(" "));
            }
            Line::from(spans)
        })
        .collect()
}

/// One row per Function and nothing else: everything the pane says about the
/// figure itself is on the border, so no row inside them is chrome and the
/// pane's row count is its height less its two border rows. Split out of
/// `risk_widget` for the reason `tree_lines` is — a `Paragraph` will not give
/// its text back.
fn risk_lines(state: &State, width: u16) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(2) as usize;
    varde::risk::list(state)
        .iter()
        .enumerate()
        .map(|(index, function)| {
            // The delta beside the figure in Review view, signed: which part of
            // the change added the risk is the question the rows are there to
            // answer, and the figure alone does not say.
            let figure = match varde::risk::row_delta(state, function) {
                Some(delta) => format!("{} {delta:+}", function.metrics.cyclomatic),
                None => function.metrics.cyclomatic.to_string(),
            };
            // The row the keyboard is on, marked the way the spine marks its
            // own: the border names its file, so the two have to agree about
            // which row that is.
            let on_this_row = index == state.risk_selection;
            let selected = if on_this_row {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            // Only that row's actions are drawn, and they take the columns
            // `mouse::risk_action_at` hit-tests: two per icon, hard against the
            // right-hand border, with the figure shifted left to make room.
            let actions = if on_this_row {
                varde::risk::row_actions(state)
            } else {
                Vec::new()
            };
            // Where the Function is, leading the figure group: acting on a row
            // means going there, and the row already knew the line. It leads
            // for the reason the review title's partial marker does — a row is
            // truncated from the right, so what must survive a narrow pane goes
            // left — and it is dimmed, so it is not read as a second figure.
            let at_line = format!(":{} ", function.line);
            // A leading space, the name, then the figures: the figures line up
            // down one column rather than trailing each name at a different
            // width.
            let room = inner.saturating_sub(
                at_line.chars().count() + figure.chars().count() + 1 + actions.len() * 2,
            );
            // With no columns left for it the name is not drawn at all:
            // `truncate` gives back an ellipsis a column wide even for no room,
            // and one column over the inner width slides the action icons off
            // the columns `mouse::risk_action_at` hit-tests them from.
            let mut spans = Vec::new();
            if room > 0 {
                spans.push(Span::styled(
                    format!(" {:<room$}", truncate(&function.name, room), room = room),
                    selected,
                ));
            }
            spans.push(Span::styled(at_line, selected.fg(Color::DarkGray)));
            spans.push(Span::styled(figure, selected.fg(WARNING)));
            for (at, action) in actions.iter().enumerate() {
                spans.push(Span::styled(
                    action_icon(action),
                    action_style(state, action, state.selected_action == Some(at)),
                ));
                spans.push(Span::raw(" "));
            }
            Line::from(spans)
        })
        .collect()
}

fn tree_widget(state: &State, rows: &[Row], width: u16) -> Paragraph<'static> {
    if filter::view_state(state) == "no-matches" {
        return Paragraph::new(Line::from(Span::styled(
            "  no files match",
            Style::default().fg(Color::DarkGray),
        )))
        .block(pane_block(tree_title(state), state, Pane::Tree));
    }
    Paragraph::new(tree_lines(state, rows, width))
        .scroll((state.tree_scroll as u16, 0))
        .block(pane_block(tree_title(state), state, Pane::Tree))
}

/// The tree's rows as styled lines. Split out of `tree_widget` because a
/// `Paragraph` will not give its text back, and the promise that only the glyph
/// carries a colour — never the filename — is checkable only on the spans.
fn tree_lines(state: &State, rows: &[Row], width: u16) -> Vec<Line<'static>> {
    rows.iter()
        .map(|row| {
            let depth = row
                .path
                .strip_prefix(&state.root)
                .map_or(0, |rest| rest.components().count().saturating_sub(1));
            let marker = match (row.is_dir, row.expanded) {
                (true, true) => "▾ ",
                (true, false) => "▸ ",
                (false, _) => "  ",
            };
            let name = row
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (icon, kind) = varde::tree::icon(&name, row.is_dir, row.expanded);
            // What is open, shown in the list of files you already have.
            let (glyph, mark_colour) = glyph(mark(state, &row.path));
            let mut style = if row.dimmed {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            let selected = state.tree_selection.as_deref() == Some(row.path.as_path());
            if selected {
                style = style.add_modifier(Modifier::REVERSED);
            }
            let indent = format!("{}{marker}", "  ".repeat(depth));
            // Only the glyph is coloured; the name keeps the row's own style, so
            // the eye reads a column of icons rather than a rainbow of names. A
            // selected or dimmed row drops the colour — REVERSED would paint the
            // row's background in it, and dimmed means dimmed.
            let icon_style = if selected || row.dimmed {
                style
            } else {
                style.fg(icon_colour(kind))
            };
            let name = format!(" {name}");

            // Action icons sit right-aligned on the focused row, at the columns
            // action_at() hit-tests against.
            let marker_span = Span::styled(glyph, Style::default().fg(mark_colour));
            let actions = tree::row_actions(state, &row.path);
            if actions.is_empty() {
                return Line::from(vec![
                    marker_span,
                    Span::styled(indent, style),
                    Span::styled(icon, icon_style),
                    Span::styled(name, style),
                ]);
            }
            let inner = width.saturating_sub(3) as usize;
            let reserved = actions.len() * 2;
            // The padding that reserves those columns rides on the name, the last
            // span: the label is three spans now, and padding any earlier one
            // would push the action icons off the columns they are hit-tested at.
            let used = indent.chars().count() + 1;
            let name = format!(
                "{name:<width$}",
                width = inner.saturating_sub(reserved).saturating_sub(used)
            );
            let mut spans = vec![
                marker_span,
                Span::styled(indent, style),
                Span::styled(icon, icon_style),
                Span::styled(name, style),
            ];
            for (index, action) in actions.iter().enumerate() {
                // Only the glyph is styled — a box around glyph-plus-space reads
                // as lopsided padding.
                spans.push(Span::styled(
                    action_icon(action),
                    action_style(state, action, state.selected_action == Some(index)),
                ));
                spans.push(Span::raw(" "));
            }
            Line::from(spans)
        })
        .collect()
}

/// A box on the tree's bottom edge, closed by the pane's own border.
fn filter_box(frame: &mut Frame, state: &State, area: Rect, draft: Option<&str>) {
    if area.height < 4 {
        return;
    }
    // The box is always there; `/` puts the keyboard in it.
    let active = draft.is_some();
    let draft = draft.unwrap_or(&state.filter);
    let inner = area.width.saturating_sub(2);
    // Only the interior: the pane's own border cells are left alone, so the
    // vertical line runs through untouched and there is no join glyph to style.
    let rule = Rect {
        x: area.x + 1,
        y: area.bottom().saturating_sub(3),
        width: inner,
        height: 1,
    };
    let field = Rect {
        x: area.x + 1,
        y: area.bottom().saturating_sub(2),
        width: inner,
        height: 1,
    };
    frame.render_widget(Clear, rule);
    frame.render_widget(
        Paragraph::new("─".repeat(inner as usize)).style(Style::default().fg(Color::DarkGray)),
        rule,
    );
    frame.render_widget(Clear, field);

    // The rest of the best match, shown ahead of the cursor.
    let completion = filter::best(state)
        .map(|best| best.rsplit('/').next().unwrap_or(&best).to_string())
        .filter(|name| {
            name.to_lowercase().starts_with(&draft.to_lowercase()) && name.len() > draft.len()
        })
        .map(|name| name[draft.len()..].to_string())
        .unwrap_or_default();
    let hint = if active || !draft.is_empty() {
        String::new()
    } else {
        "filter files".to_string()
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "/",
                Style::default().fg(if active { Color::Cyan } else { Color::DarkGray }),
            ),
            Span::raw(draft.to_string()),
            Span::styled(completion, Style::default().fg(Color::DarkGray)),
            Span::styled(hint, Style::default().fg(Color::DarkGray)),
        ])),
        field,
    );
    if active {
        frame.set_cursor_position((field.x + 1 + draft.chars().count() as u16, field.y));
    }
}

fn review_widget(state: &State, width: u16) -> Paragraph<'static> {
    let rows = tree::visible_rows(state);
    let lines = match rows.is_empty() {
        true => match review::view_state(state) {
            "not-a-git-repository" => "Not a git repository.\nReview needs git.",
            _ => "Working tree clean.\nNothing to review.",
        }
        .lines()
        .map(Line::from)
        .collect(),
        false => review_lines(state, &rows, width),
    };
    Paragraph::new(lines)
        .scroll((state.tree_scroll as u16, 0))
        .block(pane_block(review_title(state), state, Pane::Tree))
}

/// One row per changed file, and what its server says about it. Split out of
/// `review_widget` for the reason `risk_lines` is split out of `risk_widget`.
fn review_lines(state: &State, rows: &[tree::Row], width: u16) -> Vec<Line<'static>> {
    // Only where they were measured. Story view lists the same changed files in
    // this pane, and its servers were told about none of them: a column of
    // "nothing known" down a pane that is not asking the question is noise.
    let counts = match state.view {
        View::Review => review::diagnostics(state),
        _ => Vec::new(),
    };
    rows.iter()
        .map(|row| {
            let name = varde::relative(state, &row.path);
            // Without this you cannot see which file you are on, which reads as
            // selection not working at all.
            let style = match state.tree_selection.as_deref() == Some(row.path.as_path()) {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
            // Matched on the path rather than on the name derived above: the
            // two are the same file, and comparing the strings would make a
            // path no `to_string_lossy` survives read as a file nobody
            // measured. Never a zero where nothing was measured — the one thing
            // this pane must not tell a reviewer — so an absence is drawn
            // rather than filled in.
            let figure = counts
                .iter()
                .find(|(file, _)| state.root.join(file) == row.path)
                .map(|(_, counts)| match counts {
                    None => "—".to_string(),
                    Some(counts) => format!("{}E {}W", counts.errors, counts.warnings),
                });
            // A row with no figure at all is a row that is not a file under
            // review: the tree lists the folders they sit in too, and Story
            // view lists the same files with nothing measured for any of them.
            let Some(figure) = figure else {
                return Line::from(Span::styled(name, style));
            };
            // Hard against the right-hand border, so the figures line up down
            // one column rather than trailing each name at a different width —
            // the Risk list's rule, for the same reason.
            let room = (width as usize)
                .saturating_sub(2)
                .saturating_sub(figure.width() + 1);
            let name = truncate(&name, room);
            let gap = room.saturating_sub(name.width()) + 1;
            Line::from(vec![
                Span::styled(name, style),
                Span::raw(" ".repeat(gap)),
                Span::styled(figure, Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect()
}

/// The pane's name and what the change as a whole carries — the first question
/// a reviewer has, answered before a line of the diff is read. On the border
/// for the reason the Risk figure is: one place on screen always answers the
/// same question.
///
/// The leading `~` is a total that does not cover every file under review, and
/// it leads for a reason: a border is truncated from the right, so a marker
/// after the figures is the half that a narrow pane drops — leaving exactly the
/// bare `0E 0W` this exists to stop being read as "the change is clean".
///
/// In Review view only. Story view lists the same changed files in this pane
/// and measures none of them, so a figure there would be a claim about a
/// question that pane is not asking.
fn review_title(state: &State) -> String {
    if state.view != View::Review {
        return "review".to_string();
    }
    match review::diagnostic_total(state) {
        review::Total::Unmeasured => "review".to_string(),
        review::Total::Whole(total) => format!("review  {}E {}W", total.errors, total.warnings),
        review::Total::Partial(total) => format!("review  ~{}E {}W", total.errors, total.warnings),
    }
}

/// Where the branch picker left the reviewer, said on the pane's border rather
/// than as a row of the spine: the spine's rows are hit-tested by index
/// (`story::title_rows`), so a line added there is a click landing on the wrong
/// Story. Both branches, because Varde does not check the original one back out
/// — see [`State::left_branch`].
fn story_title(state: &State) -> String {
    let Some(left) = &state.left_branch else {
        return "story".to_string();
    };
    // Where the reviewer is comes off the poll rather than out of the pick, so
    // a branch changed in the terminal pane changes what this says.
    let on = state.branch.as_deref().unwrap_or("a detached HEAD");
    format!("story  on {on} (was {left})")
}

/// Each Story by name, with its Step count — the shape of the change, before
/// any of it is walked. The same pane Review view lists changed files in.
fn spine_widget(state: &State) -> Paragraph<'static> {
    let rows = story::spine(state);
    if !rows.is_empty() {
        let mut lines: Vec<Line> = Vec::new();
        if let Some(title) = story::title(state) {
            lines.push(Line::from(Span::styled(
                title.to_string(),
                Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
        }
        lines.extend(rows.iter().enumerate().map(|(index, row)| {
            let style = if index == state.story_selection {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let text = if row.stale > 0 {
                format!("  {} ({} steps, {} stale)", row.name, row.steps, row.stale)
            } else {
                format!("  {} ({} steps)", row.name, row.steps)
            };
            Line::from(Span::styled(text, style))
        }));
        lines.push(Line::from(""));
        // Selectable only once there is something in it to walk — a
        // reviewer cannot enter a Remainder with nothing left in it.
        let remainder_selected =
            !story::remainder_locations(state).is_empty() && state.story_selection >= rows.len();
        lines.extend(remainder_lines(
            &story::remainder(state),
            remainder_selected,
        ));
        return Paragraph::new(lines)
            .scroll((state.spine_scroll as u16, 0))
            .block(pane_block(story_title(state), state, Pane::Tree));
    }
    let message = match &state.story_set {
        story::Set::None => "Nothing authored yet.\nStory view has no story to walk.".to_string(),
        story::Set::RangeGone => {
            "This story set's range no longer resolves.\nRe-author it to walk it.".to_string()
        }
        story::Set::NoDefaultBranch => {
            "Nothing to resolve `:story` against.\nVarde could not find a default branch offline."
                .to_string()
        }
        story::Set::BadRange => "That range does not resolve.\nCheck the spelling.".to_string(),
        story::Set::NotARepository => {
            "This folder is not a git repository.\nThere are no branches to story.".to_string()
        }
        // The instruction is the whole message: a refusal that does not say
        // what to do about it is a refusal the reviewer reads twice.
        story::Set::WorkingTreeDirty => {
            "The working tree has uncommitted changes.\nCommit them first — picking a branch \
             checks it out."
                .to_string()
        }
        story::Set::GuestNeedsBareWorkspace => {
            "This workspace is a project.\nRun `varde` with no folder to review a repository \
             that is not on this machine."
                .to_string()
        }
        story::Set::NoGit => {
            "Cloning needs `git` on this machine.\nVarde clones with your own git so your SSH \
             config works."
                .to_string()
        }
        // No timeout here either, and no spinner: the download is running in
        // the shell pane, where its own progress is on screen (ADR 0015).
        story::Set::Downloading { url, how } => {
            let doing = match how {
                story::Download::Clone => "Cloning",
                story::Download::Fetch => "Fetching",
            };
            format!("{doing} {url} in the terminal below…")
        }
        // The status, because git fails for reasons only git knows and the
        // pane below has just printed them.
        story::Set::DownloadFailed { how, status } => {
            let what = match how {
                story::Download::Clone => "clone",
                story::Download::Fetch => "fetch",
            };
            match status {
                Some(status) => format!(
                    "The {what} failed — git exited {status}.\nWhat it printed is in the \
                     terminal below."
                ),
                None => format!(
                    "The {what} failed — its exit status could not be read.\nWhat git printed \
                     is in the terminal below."
                ),
            }
        }
        // Untrusted text, straight into a Span rather than a raw write: a
        // ratatui Span never interprets an escape a broken artifact's error
        // text might carry.
        story::Set::Refused { because } => format!("The story set could not be read:\n{because}"),
        story::Set::Loaded(_) => "The story set holds no stories.".to_string(),
        // No timeout: it says it is waiting because that is what is true,
        // never a guessed duration (ADR 0006).
        story::Set::Authoring { spelling } => format!("Authoring a story for \"{spelling}\"…"),
        // A round trip inside one batch of input, so this is on screen only
        // if the read is slow — but it says what is true rather than showing
        // an empty spine (ticket 04).
        story::Set::Filling { .. } => "Reading what the story's sites hold…".to_string(),
        // Distinct from the authoring wait on purpose (ticket 06): a reviewer
        // who cannot tell a second round from a slow first one has no way to
        // know whether waiting longer is reasonable. The round is named, not a
        // duration — there is still no clock here. Untrusted step ids, so
        // straight into a Span like the refusal above.
        story::Set::Fixing {
            attempt, problems, ..
        } => format!(
            "Fixing the story set — round {} of {}. What did not hold up:\n{}",
            attempt,
            story::ATTEMPTS,
            story::refusal(problems),
        ),
        story::Set::AuthoringAbandoned => {
            "The AI exited before the story arrived.\nNothing was authored.".to_string()
        }
    };
    Paragraph::new(message).block(pane_block(story_title(state), state, Pane::Tree))
}

/// The narration band: the current Step's claim, under a separator that
/// marks it off from the code above, and — only when the Step has them — a
/// row of its cited values. Takes no focus and is drawn into its own `Area`
/// — never computed here, so drawing and hit-testing cannot disagree about
/// where it is (`layout::panes`). The values row takes one of the band's
/// fixed content rows rather than adding one, so the code never moves under
/// a keypress.
fn band_widget(state: &State) -> Paragraph<'static> {
    let step = story::current_step(state);
    // The warning rides the claim's own line rather than adding one: the
    // band's height is fixed (`story::BAND_HEIGHT`) so the code above it
    // never moves under a keypress, and a values row already claims the one
    // spare content row a Step can have.
    let claim_line = match step {
        Some(step) if story::staleness(state, step).is_stale() => Line::from(vec![
            Span::styled(
                "STALE  ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(step.claim.clone()),
        ]),
        Some(step) => Line::from(step.claim.clone()),
        // Reached only while walking the Remainder: a Story's Step always
        // resolves to `Some` here. Said plainly rather than left blank,
        // since a blank line reads as an empty claim, not as no claim at
        // all — nobody authored one for a hunk no Step reached.
        None => Line::from(Span::styled(
            "No claim — nothing here was authored.",
            Style::default().fg(Color::DarkGray),
        )),
    };
    let mut lines = vec![claim_line];
    if let Some(step) = step {
        if !step.values.is_empty() {
            lines.push(values_line(&step.values));
        }
    }
    Paragraph::new(lines).block(Block::default().borders(Borders::TOP))
}

/// One line naming every one of a Step's cited values. An `invented` value —
/// one with nowhere to point — is coloured apart from a cited one, since that
/// distinction is the whole reason a reviewer can trust the rest.
fn values_line(values: &[story::Value]) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        let displayed = value.displayed_provenance();
        let colour = if displayed == story::Provenance::Invented.as_str() {
            Color::Yellow
        } else {
            Color::Cyan
        };
        spans.push(Span::styled(
            format!("{}: {} [{}]", value.name, value.value, displayed),
            Style::default().fg(colour),
        ));
    }
    Line::from(spans)
}

/// The step-menu: every Step of the Story being walked, in order, with the
/// current one marked — a spatial index, not a control, drawn into its own
/// `Area` the same way the band is. An over-length name truncates with an
/// ellipsis rather than wrapping, which would make the menu's row count
/// disagree with the Story's step count.
fn step_menu_widget(state: &State, width: u16) -> Paragraph<'static> {
    let rows = story::step_menu(state);
    let inner = width.saturating_sub(1) as usize;
    let lines: Vec<Line<'static>> = rows
        .into_iter()
        .map(|row| {
            let style = if row.current {
                Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default()
            };
            Line::from(Span::styled(truncate(&row.name, inner), style))
        })
        .collect();
    Paragraph::new(lines).block(Block::default().borders(Borders::RIGHT))
}

/// A name that fits `width` display columns unchanged, or one truncated with
/// a trailing ellipsis so it never wraps onto a second row. Cut by display
/// width via [`UnicodeWidthChar`], not by `chars().count()` — a wide glyph
/// split on a character boundary would still overrun the column it was cut to
/// fit.
fn truncate(name: &str, width: usize) -> String {
    if name.width() <= width {
        return name.to_string();
    }
    let kept = width.saturating_sub(1);
    let mut remaining = kept;
    let mut truncated = String::new();
    for ch in name.chars() {
        let w = ch.width().unwrap_or(0);
        if w > remaining {
            break;
        }
        truncated.push(ch);
        remaining -= w;
    }
    truncated.push('…');
    truncated
}

/// The Remainder under the Stories: a count and a list, never a ratio — and
/// the deletions line only when the range deletes something, so a reviewer
/// can tell "nothing was deleted" from "deletions nobody walked". `selected`
/// highlights the count line the same way a selected Story row is
/// highlighted, since Enter on it walks the Remainder the same way Enter on
/// a Story row walks that Story — but only that one line, never the list
/// under it, which is not a row of its own.
fn remainder_lines(remainder: &story::Remainder, selected: bool) -> Vec<Line<'static>> {
    let noun = if remainder.unclaimed == 1 {
        "hunk"
    } else {
        "hunks"
    };
    let style = if selected {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    let mut lines = vec![Line::from(Span::styled(
        format!("  Remainder: {} unclaimed {noun}", remainder.unclaimed),
        style,
    ))];
    lines.extend(
        remainder
            .locations
            .iter()
            .map(|location| Line::from(format!("    {location}"))),
    );
    if let Some(unwalked) = remainder.unwalked_deletions {
        lines.push(Line::from(format!("  Deletions: {unwalked} unwalked")));
    }
    lines
}

fn editor_widget(
    state: &State,
    command: Option<&str>,
    (tokens, run_marks): (&[Vec<highlight::Token>], &[usize]),
    diff_sides: (&[Vec<highlight::Token>], &[Vec<highlight::Token>]),
    preview: &[varde::preview::Row],
    area: Rect,
    faint: Style,
) -> Paragraph<'static> {
    let width = area.width;
    // Both of these substitute the whole drawing of the editor's rectangle
    // rather than being a `Pane` of their own. They never coexist:
    // `move_to_view` empties `state.diff` on every way out of
    // Review and ends the walk on every way out of Story.
    if state.walking.is_some() {
        return story_widget(state, command, tokens, width);
    }
    if let (Some(diff), Some(file)) = (&state.diff, &state.diff_file) {
        return diff_widget(state, diff, file, diff_sides.0, diff_sides.1);
    }
    let Some((path, buffer)) = state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path).map(|buffer| (path, buffer)))
    else {
        // No file open still needs the command line: `:q` has to work — and a
        // refusal earned with nothing open has nowhere else to be said.
        return Paragraph::new("").block(editor_block(
            state,
            "editor".into(),
            refusal_spans(state),
            command,
            width,
        ));
    };

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = buffer_title(
        &name,
        buffer,
        varde::mode_label(state, buffer),
        title_room(state, width),
    );
    // Half-typed commands and the cursor position sit along the bottom edge,
    // where vim puts them.
    let mut footer = vec![Span::styled(
        format!(" {} ", buffer.pending_command()),
        Style::default().fg(Color::Cyan),
    )];
    footer.extend(dot_spans(state));
    // The Preview's own pair while it is showing: a source line and column
    // reported over rendered text names a place that is not on screen, which
    // [`preview_widget`] has always said it does not do.
    let (line, column) = if varde::previewing(state) {
        (buffer.row, buffer.row_column)
    } else {
        (buffer.line, buffer.column)
    };
    footer.push(Span::styled(
        varde::mouse::position_label(line, column),
        Style::default().fg(Color::Cyan),
    ));
    footer.extend(refusal_spans(state));
    // Why the line under the cursor is marked, said where the position already
    // is. A mark whose reason costs a keystroke to read is a mark the reader
    // learns to ignore.
    if let Some(message) = lsp::message_at_cursor(state) {
        footer.push(Span::styled(
            format!(" {message} "),
            Style::default().fg(Color::Red),
        ));
    }

    if varde::previewing(state) {
        return preview_widget(state, command, title, footer, preview, width);
    }

    Paragraph::new(source_lines(
        state,
        path,
        buffer,
        (tokens, run_marks),
        area,
        faint,
    ))
    .block(editor_block(state, title, footer, command, width))
}

/// The buffer's rows the pane has room for, and nothing else. Which lines
/// those are is settled first — the scroll offset, the pane's height and the
/// folds decide it, through the same [`story::rows`] the caret and a click
/// count with — and only they are built and marked. Built for the whole file
/// and then scrolled, a frame cost as much as the file was long (#101).
///
/// Split out of `editor_widget` for the reason `risk_lines` is: a `Paragraph`
/// will not give its text back, and how many lines it was handed is what this
/// promises.
fn source_lines(
    state: &State,
    path: &std::path::Path,
    buffer: &varde::editor::Buffer,
    (tokens, run_marks): (&[Vec<highlight::Token>], &[usize]),
    area: Rect,
    faint: Style,
) -> Vec<Line<'static>> {
    let shown: Vec<usize> = story::rows(state, tokens.len())
        .skip(state.editor_scroll)
        .take(area.height.saturating_sub(2) as usize)
        .filter_map(|row| match row {
            story::Row::Code(number) => Some(number as usize),
            _ => None,
        })
        .collect();
    let (Some(&first), Some(&last)) = (shown.first(), shown.last()) else {
        return Vec::new();
    };
    // Which of the drawn rows a line is on, if it is drawn at all. Every pass
    // below finds a line by its number, and a fold means a number is not an
    // offset.
    let row = |number: usize| shown.binary_search(&number).ok();
    let dark = state.editor_theme != "light";
    let mut lines = code_lines(tokens, shown.iter().copied(), dark, cursor_line(state));
    // Under everything below, the way an indentation guide is under the code
    // in an IDE: each of the passes below counts columns, and substituting a
    // character for a glyph leaves every column where it was.
    // Quiet enough to read as paper rather than as content: a guide competing
    // with the syntax under it is worse than no guide. The cursor's block is
    // the only one drawn to be noticed.
    let bracket = Style::default().bg(if dark {
        Color::Indexed(236)
    } else {
        Color::Indexed(254)
    });
    let guides = buffer.guides(shown.iter().copied());
    // The one pair the cursor is inside, whose two halves are on one line or on
    // two. Which pair that is is the library's answer; the columns of it that
    // fall on this row are all this needs.
    let pair = buffer.bracket_pair();
    for ((line, number), guides) in lines.iter_mut().zip(&shown).zip(&guides) {
        let marks: Vec<usize> = pair
            .iter()
            .flat_map(|(from, to)| [from, to])
            .filter(|at| at.line == *number)
            .map(|at| at.column - 1)
            .collect();
        *line = guided(line, guides, &marks, bracket, faint);
    }
    // Where the word under the cursor is used, washed rather than picked: it is
    // not a selection, and a mark as bright as one would read as text having
    // been chosen by a cursor that only came to rest. Under the selection, the
    // echoes and the search marks, so every one of them still reads brighter.
    let word = buffer
        .word_at_cursor()
        .map_or(0, |word| word.chars().count());
    for at in varde::word_occurrences(state, first..=last) {
        let Some(line) = row(at.line).map(|index| &mut lines[index]) else {
            continue;
        };
        *line = picked(
            line,
            at.column - 1,
            at.column - 1 + word,
            Style::default().bg(word_tint(dark)),
            true,
        );
    }
    // The expression the Hover is a claim about, washed for as long as the
    // box is up: a box floating over the code says what a value is without
    // saying what it was read off, and the reader is left to guess whether
    // the field or the whole chain was evaluated.
    if let Some((at, width)) = varde::debug::hover_span(state) {
        if let Some(line) = row(at.line).map(|index| &mut lines[index]) {
            *line = picked(
                line,
                at.column - 1,
                at.column - 1 + width,
                Style::default().bg(word_tint(dark)),
                true,
            );
        }
    }
    // Without this a visual selection is invisible, which reads as V not
    // working at all.
    if let Some((from, to)) = buffer.selected_lines() {
        for (line, _) in lines
            .iter_mut()
            .zip(&shown)
            .filter(|(_, number)| (from..=to).contains(*number))
        {
            line.style = line.style.add_modifier(Modifier::REVERSED);
        }
    }
    // Every other place the picked word is used, quieter than the pick and under
    // both it and the search marks: an echo is a hint about the file, not a
    // second selection.
    let echoed = varde::echoes(state, first..=last);
    if let Some(word) = state.selected_text().filter(|_| !echoed.is_empty()) {
        let width = word.chars().count();
        for at in echoed {
            let Some(line) = row(at.line).map(|index| &mut lines[index]) else {
                continue;
            };
            *line = picked(
                line,
                at.column - 1,
                at.column - 1 + width,
                Style::default().bg(echo_tint(state.editor_theme != "light")),
                true,
            );
        }
    }
    // Every match of what `/` looked for, not only the one the cursor is on, so
    // the count is visible without walking them. Under the selection, so the
    // match being stepped to still reads as picked.
    let matched = state
        .find
        .as_ref()
        .map_or(0, |find| find.query.shown().chars().count());
    for at in varde::matches(state, first..=last) {
        let Some(line) = row(at.line).map(|index| &mut lines[index]) else {
            continue;
        };
        *line = picked(
            line,
            at.column - 1,
            at.column - 1 + matched,
            Style::default().bg(Color::Yellow).fg(Color::Black),
            true,
        );
    }
    // Which characters the server pointed at, under the code and not beside
    // it: the gutter's one column says a line is wrong, and on a line holding
    // three calls that leaves the reader guessing which one. The colour is the
    // underline's alone — `picked` patches, so the syntax colour underneath is
    // still the one the text is drawn in, the way an editor marks a span rather
    // than repainting it.
    for (line, number) in lines.iter_mut().zip(&shown) {
        for (from, to, severity) in lsp::underlines(state, path, *number) {
            *line = picked(
                line,
                from - 1,
                to,
                Style::default()
                    .add_modifier(Modifier::UNDERLINED)
                    .underline_color(severity_colour(severity)),
                true,
            );
        }
    }
    paint_drag(
        &mut lines,
        &row,
        state.selection.as_ref().and_then(Selection::buffer_span),
        &state.occurrences,
        true,
    );
    // The whole affordance a link has: a terminal has no hand pointer to turn
    // the mouse into, so the underline is what says a click here jumps.
    if let Some((line, from, to)) = varde::link(state) {
        if let Some(drawn) = row(line).map(|index| &mut lines[index]) {
            *drawn = picked(
                drawn,
                from - 1,
                to,
                Style::default().add_modifier(Modifier::UNDERLINED),
                true,
            );
        }
    }
    // What the program holds, at the end of the lines the Paused call has
    // already run. Faint, the way the indentation guides are faint: a value is
    // a note about the code and never part of it. A value that moved at this
    // pause is the one thing here worth looking at, so it is the one thing
    // drawn in the foreground's own colour — the faintness is what the others
    // are for. Appended after every pass that counts columns from the start of
    // a line, so none of them can reach into a span that is not the file's
    // text.
    for (number, values) in varde::debug::inline(state, tokens, varde::fits(state).2) {
        let Some(line) = row(number).map(|index| &mut lines[index]) else {
            continue;
        };
        for value in values {
            // No colour of its own: the text's own foreground, bold, beside
            // neighbours mixed to a tenth of it. A hue here would be a fourth
            // palette to keep in step with the themes.
            let style = match value.changed {
                true => Style::default().add_modifier(Modifier::BOLD),
                false => faint,
            };
            line.spans.push(Span::styled(value.text, style));
        }
    }
    shift(&mut lines, state, 1);
    // A Conflict's marker lines as bars and its sides tinted — after `shift`,
    // so a bar is never scrolled sideways away from its buttons, and before
    // the gutter's marks, which still count the line.
    for (line, &number) in lines.iter_mut().zip(&shown) {
        let Some(drawn) = varde::conflict::drawn(state, number) else {
            continue;
        };
        *line = match drawn {
            varde::conflict::Drawn::Bar(pieces) => {
                let text: String = pieces.into_iter().map(|(piece, _)| piece).collect();
                // The width `conflict::drawn` measured the bar against.
                let run = varde::fits(state).2.saturating_sub(text.chars().count());
                let mut bar = numbered(number, cursor_line(state));
                bar.push_span(Span::styled(
                    format!("{text}{}", "┄".repeat(run)),
                    Style::default().add_modifier(Modifier::DIM),
                ));
                bar
            }
            varde::conflict::Drawn::Current => washed(line, change_colours(false, dark).2),
            varde::conflict::Drawn::Incoming => washed(line, incoming_tint(dark)),
            varde::conflict::Drawn::Ancestor => washed(line, word_tint(dark)),
        };
    }
    // Last, for the reason the Site's mark is last in `marked_code`: everything
    // above counts columns from the start of the line, and a barred line's
    // gutter is two spans rather than one.
    //
    // The three marks share the one column the gutter has: a line the server
    // has something to say about keeps saying it, because a diagnostic is news
    // and where the voice has got to is only where the voice has got to; a
    // change bar is the quietest of the three, since the change is already on
    // the line. The wash goes on either way, so the passage is still whole.
    let spoken = reading::mark(state);
    let changed = varde::changed_lines(state);
    for (line, &number) in lines.iter_mut().zip(&shown) {
        let reading = spoken.is_some_and(|(from, to)| number >= from && number <= to);
        if let Some(severity) = lsp::mark(state, path, number) {
            *line = barred(line, number, severity_colour(severity));
        } else if reading {
            *line = barred(line, number, READING);
        } else if changed.binary_search(&number).is_ok() {
            *line = barred(line, number, Color::Green);
        }
        if reading {
            *line = washed(line, reading_tint(state.editor_theme != "light"));
        }
    }

    // The fold toggles, last of everything the editor draws for the reason
    // the Site's mark is last: everything above counts columns from the start
    // of the line. The toggle takes `layout::TOGGLE_COLUMN` of the gutter's
    // pad, so text still starts where `layout::GUTTER` says it does and a drag
    // lands on the character it is over; a line the language server or a
    // Reading has marked keeps that mark too — the bar is in the column
    // before, which is what the seventh gutter column was for. The Breakpoint
    // marks go after it, since the toggle recognises a line's gutter by its
    // first span.
    let toggles = varde::fold::toggles(state, first..=last);
    let marks = varde::debug::marks(state);
    let paused = varde::debug::paused_line(state)
        .filter(|(file, _, _)| state.current_buffer.as_deref() == Some(*file));
    lines
        .into_iter()
        .zip(shown)
        .map(|(line, number)| {
            let line = match toggles.get(&number) {
                Some(toggle) => with_toggle(&line, number, *toggle, dark),
                None => line,
            };
            // A Breakpoint over a Run mark, as a click in the column takes it.
            let line = match marks.get(&number) {
                Some(mark) => with_breakpoint(line, *mark),
                None if run_marks.contains(&number) => with_run_mark(line),
                None => line,
            };
            match paused {
                Some((_, at, why)) if at == number => on_paused_line(line, why, area.width, dark),
                _ => line,
            }
        })
        .collect()
}

/// The mirror of the file down the editor's right-hand edge, the mark lane
/// down its last column, and the scrollbar when there is no mirror.
///
/// Four source columns and two source lines to a cell, drawn as a dot: a
/// bullet where both lines hold ink, a middle dot where one does. Why a dot
/// rather than a block, and why these two glyphs rather than braille, is
/// argued in `varde::minimap` — it is the difference between a miniature and a
/// wall of slabs.
///
/// One colour per cell, the upper line's where it has ink: a cell has one
/// foreground. Dimmed rather than recoloured — `Modifier::DIM` is the only
/// thing a terminal has for low opacity, and it fades whatever the theme's own
/// colour is, which keeps the mirror a mirror instead of a second palette to
/// maintain.
///
/// The slider is a **line down the strip's first column**, beside the rows the
/// editor's own window covers. A wash behind those rows was the first try, and
/// what it actually drew was a grey block in every cell the code did not fill:
/// a mirror this faint has more blank than ink, so a field behind it is the
/// loudest thing on the pane. The one column it costs is the cheapest thing
/// there is to spend, and the mirror keeps the editor's own background
/// everywhere else.
fn minimap(frame: &mut Frame, state: &State, areas: &Areas, tokens: &[Vec<highlight::Token>]) {
    let fits = varde::fits(state).1;
    let dark = state.editor_theme != "light";
    let area = areas.minimap;
    if let Some((first, _)) = minimap::mirrored(state) {
        // The field the editor itself paints, so a blank cell of the mirror is
        // the colour blank code is — and opaque either way, since the text
        // underneath has to be covered.
        let field = match state.editor_field && dark {
            true => Color::Black,
            false => Color::Reset,
        };
        // Lit under the pointer, which is also the whole of the gesture: you
        // travel by holding the strip, so the line the hand is on is the line
        // that brightens and stays bright until the hand leaves. Brightened to
        // the foreground itself rather than to a second slot, which would be
        // the palette this layer stopped guessing at.
        let slider = match minimap::lit(state) {
            true => Style::new().fg(Color::Reset),
            false => FAINT,
        };
        frame
            .buffer_mut()
            .set_style(area, Style::default().bg(field));
        // The first column is the slider's and the last the lane's, so the
        // cells are the ones between.
        let (top, height) = minimap::slider(first - 1, state.editor_scroll, fits);
        let cells = area.width.saturating_sub(2) as usize;
        let lane = minimap::lane(state, first - 1, area.height as usize);
        let rows: Vec<Line<'static>> =
            minimap::cells(tokens, first - 1, area.height as usize, cells)
                .into_iter()
                .zip(lane)
                .enumerate()
                .map(|(row, (cells, mark))| {
                    let inside = row >= top && row < top + height;
                    let mut spans = vec![Span::styled(
                        match inside {
                            true => "\u{2502}",
                            false => " ",
                        },
                        slider.bg(field),
                    )];
                    spans.extend(cells.into_iter().map(|cell| {
                        // Exhaustive on the pair rather than asking
                        // twice: a dot and a colour are one decision,
                        // and the arm with nothing in it is the one
                        // that must draw nothing rather than a faint
                        // something.
                        let (dot, kind) = match (cell.top, cell.bottom) {
                            (Some(kind), Some(_)) => ("\u{2022}", kind),
                            (Some(kind), None) | (None, Some(kind)) => ("\u{00b7}", kind),
                            // Nothing to draw: the editor's own
                            // background is what a blank cell of the
                            // mirror shows, the same as blank code.
                            (None, None) => return Span::raw(" "),
                        };
                        Span::styled(
                            dot,
                            Style::default()
                                .fg(colour(kind, dark))
                                .bg(field)
                                .add_modifier(Modifier::DIM),
                        )
                    }));
                    // The gutter's own colours, so red on the strip means what
                    // red beside a line does.
                    spans.push(match mark {
                        Some(mark) => Span::styled(
                            "\u{2595}",
                            Style::default()
                                .fg(match mark {
                                    minimap::Mark::Error => severity_colour(lsp::Severity::Error),
                                    minimap::Mark::Warning => {
                                        severity_colour(lsp::Severity::Warning)
                                    }
                                    minimap::Mark::Changed => Color::Green,
                                })
                                .bg(field),
                        ),
                        None => Span::raw(" "),
                    });
                    Line::from(spans)
                })
                .collect();
        frame.render_widget(Paragraph::new(rows), area);
    }
    // The thumb, in the pane's last column — but only with no mirror beside it.
    // One indicator at a time: with the strip up, the slider is already saying
    // where the window is, and two bars down one pane that look alike and mean
    // different things (the window inside the mirror, the window inside the
    // file) is a difference nobody can read. Over the text is where it earns
    // its keep, since there the buffer has nothing else that says so.
    // A strip with columns is a strip on screen, which is the library's answer
    // rather than a second one asked here.
    if area.width > 0 || !minimap::mirroring(state) {
        return;
    }
    let Some((top, height)) = minimap::thumb(state.editor_scroll, minimap::lines(state), fits)
    else {
        return;
    };
    // The same line the slider draws, not a painted block: a solid bar is the
    // loudest thing on the pane, which is what the wash behind the mirror was.
    let bar = Paragraph::new(vec![Line::from("\u{2502}"); height]).style(FAINT);
    frame.render_widget(
        bar,
        Rect::new(
            areas.editor.x + areas.editor.width.saturating_sub(2),
            areas.editor.y + 1 + top as u16,
            1,
            height as u16,
        ),
    );
}

/// The Breakpoint box's rows, the one the keys type into marked. The switch
/// names the scope it is set to.
fn breakpoint_box_lines(
    field: varde::debug::Field,
    draft: &varde::debug::Properties,
) -> Vec<Line<'static>> {
    varde::debug::FIELDS
        .iter()
        .map(|row| {
            let label = match row {
                varde::debug::Field::Condition => "condition",
                varde::debug::Field::HitCount => "hit count",
                varde::debug::Field::LogMessage => "log message",
                varde::debug::Field::Suspend => "suspend",
            };
            let value = match row {
                varde::debug::Field::Suspend => match draft.suspend {
                    varde::debug::Suspend::Thread => "thread  (space: all)".to_string(),
                    varde::debug::Suspend::All => "all  (space: thread)".to_string(),
                },
                _ => varde::debug::field_text(draft, *row).to_string(),
            };
            let style = match *row == field {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
            Line::from(vec![
                Span::styled(
                    format!(" {label:<12}"),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!(" {value} "), style),
            ])
        })
        .collect()
}

/// One line with its Breakpoint in the gutter's first column, which every
/// gutter leaves blank for it: `layout::BREAKPOINT_COLUMN`, where `mouse`
/// hit-tests the click that set it.
fn with_breakpoint(line: Line<'static>, mark: varde::debug::Mark) -> Line<'static> {
    let glyph = match mark {
        varde::debug::Mark::Plain => Span::styled("●", Style::default().fg(Color::Red)),
        varde::debug::Mark::Conditional => Span::styled("◉", Style::default().fg(Color::Red)),
        // A diamond, as most debuggers draw one: it prints rather than pauses.
        varde::debug::Mark::Logpoint => Span::styled("◆", Style::default().fg(Color::Yellow)),
        // Dotted and grey: remembered against text its line no longer holds,
        // so not a place the program will pause. Not the hollow circle, which
        // is an Unverified breakpoint's.
        varde::debug::Mark::Stale => Span::styled("◌", Style::default().fg(Color::DarkGray)),
        // Hollow and still red: a line the user meant, which the adapter could
        // not bind — resting the pointer on it says why.
        varde::debug::Mark::Unverified => Span::styled("○", Style::default().fg(Color::Red)),
    };
    let style = line.style;
    let mut spans = line.spans.into_iter();
    let Some(first) = spans.next() else {
        return Line::from(vec![glyph]).style(style);
    };
    let rest: String = first.content.chars().skip(1).collect();
    let mut drawn = vec![glyph, Span::styled(rest, first.style)];
    drawn.extend(spans);
    Line::from(drawn).style(style)
}

/// A Run mark's offer: what it would start, and the Run and Debug Chips on
/// the box's top border, at the columns `run::chip_at` hit-tests.
fn run_offer(frame: &mut Frame, state: &State) {
    let screen = frame.area();
    let Some(offer) = varde::run::offer_area(state, screen.width, screen.height) else {
        return;
    };
    let box_area = rect(offer);
    frame.render_widget(Clear, box_area);
    frame.render_widget(
        Paragraph::new(format!(
            "  {}",
            varde::run::offered(state).unwrap_or_default()
        ))
        .block(Block::default().borders(Borders::ALL).title("RUN")),
        box_area,
    );
    let chips = varde::run::chips(state);
    let labels = layout::chip_labels(&chips, offer.width, 0);
    let Some(mut x) =
        (offer.x + offer.width.saturating_sub(1)).checked_sub(layout::strip_width(&labels))
    else {
        return;
    };
    for (chip, label) in chips.iter().zip(labels) {
        let columns = label.width() as u16;
        frame.render_widget(
            Line::from(chip_spans(state, chip, label).to_vec()),
            Rect::new(x, offer.y, columns, 1),
        );
        x += columns + 1;
    }
}

/// One line with the ▶ of a Run mark in the Breakpoint column.
fn with_run_mark(line: Line<'static>) -> Line<'static> {
    let style = line.style;
    let mut spans = line.spans.into_iter();
    let first = spans.next().unwrap_or_default();
    let mut drawn = vec![
        Span::styled("\u{25b6}", Style::default().fg(Color::Green)),
        Span::styled(
            first.content.chars().skip(1).collect::<String>(),
            first.style,
        ),
    ];
    drawn.extend(spans);
    Line::from(drawn).style(style)
}

/// The line a Debug session is Paused on: its marker in the Breakpoint column,
/// over any Breakpoint there, and its background across the pane's whole
/// width rather than only under the text, so a short line reads as the line
/// the program is on. An exception pause is the error colour's.
fn on_paused_line(
    line: Line<'static>,
    why: varde::debug::Why,
    width: u16,
    dark: bool,
) -> Line<'static> {
    let tint = match (why, dark) {
        (varde::debug::Why::Paused, true) => Color::Rgb(0x1e, 0x33, 0x4a),
        (varde::debug::Why::Paused, false) => Color::Indexed(153),
        (varde::debug::Why::Exception, true) => Color::Rgb(0x4a, 0x1e, 0x1e),
        (varde::debug::Why::Exception, false) => Color::Indexed(224),
    };
    let marker = match why {
        varde::debug::Why::Paused => Color::Yellow,
        varde::debug::Why::Exception => Color::Red,
    };
    let mut spans = line.spans.into_iter();
    let first = spans.next().unwrap_or_default();
    let mut drawn = vec![
        Span::styled("\u{2192}", Style::default().fg(marker)),
        Span::styled(
            first.content.chars().skip(1).collect::<String>(),
            first.style,
        ),
    ];
    drawn.extend(spans);
    let used: usize = drawn.iter().map(|span| span.content.width()).sum();
    drawn.push(Span::raw(
        " ".repeat((width.saturating_sub(2) as usize).saturating_sub(used)),
    ));
    washed(&Line::from(drawn), tint)
}

/// What a folded block leaves at the end of the line that opens it: the one
/// character standing for everything hidden under it, which is also the one
/// column past the text the cursor may sit on there
/// (`editor::Buffer::on_fold_dots`) and the one a click unfolds by.
pub const DOTS: &str = "⋯";

/// One line with its fold toggle in the gutter's pad, and — while the block is
/// folded — the [`DOTS`] that stand for the lines it hides.
///
/// Two shapes of gutter to recognise, because a marked line's is three spans
/// and a plain one's is one: the toggle takes the same screen column either
/// way, which is the whole point of naming that column in `layout`.
fn with_toggle(
    line: &Line<'static>,
    number: usize,
    toggle: varde::fold::Toggle,
    dark: bool,
) -> Line<'static> {
    let glyph = Span::styled(
        match toggle {
            varde::fold::Toggle::Folded => "►",
            varde::fold::Toggle::Open => "▼",
        },
        Style::default().fg(Color::Cyan),
    );
    let first = line.spans.first().map(|span| span.content.as_ref());
    let mut spans = if first == Some(format!(" {number:>4} {PAD}").as_str()) {
        // The number keeps whatever colour it was given — the cursor's line is
        // brighter, and rebuilding it grey here is how that mark would vanish
        // on exactly the lines a reader folds.
        let mut spans = vec![
            Span::styled(format!(" {number:>4} "), line.spans[0].style),
            glyph,
            Span::raw("  "),
        ];
        spans.extend(line.spans.iter().skip(1).cloned());
        spans
    } else if first == Some(format!(" {number:>4}").as_str())
        && line.spans.get(2).map(|span| span.content.as_ref()) == Some(PAD)
    {
        let mut spans = line.spans.clone();
        spans[2] = glyph;
        spans.insert(3, Span::raw("  "));
        spans
    } else {
        return line.clone();
    };
    if toggle == varde::fold::Toggle::Folded {
        spans.push(Span::styled(DOTS, dots_style(dark)));
    }
    Line::from(spans)
}

/// The pill the folded dots are drawn as: a background rather than a colour,
/// for the reason [`reading_tint`] is one — it has to read as a thing you can
/// press without competing with the syntax it sits at the end of.
fn dots_style(dark: bool) -> Style {
    match dark {
        true => Style::default().fg(Color::Gray).bg(Color::Indexed(238)),
        false => Style::default().fg(Color::Black).bg(Color::Indexed(252)),
    }
}

/// Which of the four a mark is announcing. Colour alone: the bar takes the one
/// column the line number was already padded with, so there is no room for a
/// glyph and no scenario asserts either.
fn severity_colour(severity: lsp::Severity) -> Color {
    match severity {
        lsp::Severity::Error => Color::LightRed,
        lsp::Severity::Warning => WARNING,
        lsp::Severity::Information => Color::Blue,
        lsp::Severity::Hint => Color::DarkGray,
    }
}

/// The dim wash behind the Utterance being spoken, the other half of the mark
/// its gutter bar starts. A background rather than an inversion, for the
/// reason the diff's tints are not inversions: a whole row of reversed code is
/// unreadable, and this one is under text somebody is reading along with.
///
/// Off the 256-cube for the same reason the diff's are — the cube has nothing
/// between its dark blues and the background — and lighter than the page on a
/// light theme rather than darker, so the wash never competes with the syntax.
fn reading_tint(dark: bool) -> Color {
    if dark {
        Color::Rgb(0x22, 0x2a, 0x38)
    } else {
        Color::Indexed(254)
    }
}

/// The background behind a word's echoes. A background rather than the pick's
/// own inversion, and off the 256-cube for the reason [`reading_tint`] is: an
/// echo has to read as quieter than the pick it echoes, and a terminal has no
/// alpha channel to make it so.
///
/// A shade stronger than [`word_tint`] on either theme: an echo answers a pick
/// somebody made, a cursor's wash answers a cursor that only came to rest, and
/// the two are painted on the same rows.
fn echo_tint(dark: bool) -> Color {
    if dark {
        Color::Rgb(0x2c, 0x3c, 0x4a)
    } else {
        Color::Indexed(252)
    }
}

/// The wash behind the word the cursor rests in. Off the 256-cube on a dark
/// theme for the reason the Reading's is, and light enough on either that the
/// syntax colours underneath are still the thing being read.
fn word_tint(dark: bool) -> Color {
    if dark {
        Color::Rgb(0x33, 0x38, 0x40)
    } else {
        Color::Indexed(253)
    }
}

/// The incoming side of a Conflict: a blue of the weight the diff's added
/// green has, so neither side reads as the one that won.
fn incoming_tint(dark: bool) -> Color {
    if dark {
        Color::Rgb(0x1b, 0x2a, 0x40)
    } else {
        Color::Indexed(189)
    }
}

/// Per span rather than on the `Line`: a span carries its own background, so a
/// style set above it would show through nothing.
fn washed(line: &Line<'static>, colour: Color) -> Line<'static> {
    Line::from(
        line.spans
            .iter()
            .map(|span| Span::styled(span.content.clone(), span.style.bg(colour)))
            .collect::<Vec<Span<'static>>>(),
    )
}

/// A Preview: the rows [`varde::preview_rows`] laid out, with no line-number
/// gutter — a Preview's rows are not lines, so five columns naming numbers the
/// reader cannot act on would be five columns not spent on the document.
///
/// The cursor's row is the one part of the pane the reader can move, so it is
/// the one thing marked. The position in the footer is a row too, for the same
/// reason: a source line reported over rendered text names a place that is not
/// on screen.
fn preview_widget(
    state: &State,
    command: Option<&str>,
    title: Line<'static>,
    footer: Vec<Span<'static>>,
    rows: &[varde::preview::Row],
    width: u16,
) -> Paragraph<'static> {
    let dark = state.editor_theme != "light";
    let columns = varde::preview_columns(state);
    let mut lines: Vec<Line> = rows
        .iter()
        .map(|row| preview_line(row, dark, columns))
        .collect();
    // Every match of what `/` looked for, painted on the row it is in — the
    // same highlight Source draws, over rows rather than lines, since
    // `varde::matches` already answers in row coordinates while previewing.
    let matched = state
        .find
        .as_ref()
        .map_or(0, |find| find.query.shown().chars().count());
    for at in varde::matches(state, ..) {
        let Some(line) = lines.get_mut(at.line - 1) else {
            continue;
        };
        *line = picked(
            line,
            at.column - 1,
            at.column - 1 + matched,
            Style::default().bg(Color::Yellow).fg(Color::Black),
            false,
        );
    }
    // No gutter to skip: a Preview draws none, so its first span is text.
    paint_drag(
        &mut lines,
        &|row| Some(row - 1),
        state
            .selection
            .as_ref()
            .and_then(|selection| selection.screen_span(Pane::Editor)),
        &[],
        false,
    );
    // The wash and nothing else: a Preview draws no gutter, so a marker column
    // would be a column of text moved one to the right — and the caret, the
    // hit-test and `layout::gutter` all count on there being none.
    if let Some((from, to)) = reading::mark(state) {
        for (line, row) in lines.iter_mut().zip(rows) {
            if row.line >= from && row.line <= to {
                *line = washed(line, reading_tint(dark));
            }
        }
    }
    // No chrome to leave behind: a Preview draws no line numbers, which is the
    // same fact `layout::gutter` answers for the hit-test and the caret.
    shift(&mut lines, state, 0);
    Paragraph::new(lines)
        .scroll((state.editor_scroll as u16, 0))
        .block(editor_block(state, title, footer, command, width))
}

/// A rendered row as a drawn line, each piece styled by what it *is*. Shared
/// by the Preview pane and the hover box, which holds the same rows since a
/// server answers in markdown — two mappings would be two answers to what a
/// fence looks like, and the box would be the one that got it wrong.
///
/// A thematic break is the one row whose glyph is not in the text: it carries
/// no pieces, so it drew as a blank line and `row_style` had a colour with
/// nothing to colour it. `columns` is what it is drawn across, which is why the
/// caller passes it — the pane's own measure, or the box's inside. The glyph is
/// put on here rather than pushed into `Row::pieces` because `Row::text` is
/// read by six other things: the hover's own width is measured off it, "every
/// row is blank" is how a server saying nothing is told from a server saying
/// something, and find-in-file, the word motions and drag-copy all read it. A
/// bar of `─` in all of those is a rule that has become text.
fn preview_line(row: &varde::preview::Row, dark: bool, columns: usize) -> Line<'static> {
    if row.kind == varde::preview::RowKind::Rule {
        return Line::from(Span::styled("─".repeat(columns), row_style(row.kind, dark)));
    }
    let mut spans = Vec::new();
    if let varde::preview::RowKind::List(item) = row.kind {
        spans.push(Span::styled(list_prefix(item), row_style(row.kind, dark)));
    }
    spans.extend(
        row.pieces
            .iter()
            .map(|piece| Span::styled(piece.text.clone(), piece_style(row.kind, dark, piece))),
    );
    Line::from(spans)
}

/// A list row's indent and marker. Kept out of `Row::text` for the same reason
/// a Rule's bar is — see [`preview_line`] — so a list that draws no glyph at
/// all is what a Preview of a numbered document looked like: prose with the
/// numbers gone. A row past the item's first carries no marker (the wrapped
/// remainder, a loose paragraph), and indents to where its text started rather
/// than repeating the bullet.
fn list_prefix(item: varde::preview::ListItem) -> String {
    use varde::preview::Marker;
    let marker = match item.marker {
        Some(Marker::Bullet) => "\u{2022} ".to_string(),
        Some(Marker::Ordinal(ordinal)) => format!("{ordinal}. "),
        Some(Marker::Task(true)) => "\u{2611} ".to_string(),
        Some(Marker::Task(false)) => "\u{2610} ".to_string(),
        None => "  ".to_string(),
    };
    format!("{}{marker}", "  ".repeat(item.depth.saturating_sub(1)))
}

/// What each kind of Preview row looks like. A theme's business, which is why no
/// scenario asserts any of it — the suite asserts the kind and leaves the
/// weight, the colour and the glyph here. Every arm is named rather than caught
/// by a `_`, so the ticket that starts producing one of the kinds below has to
/// decide what it looks like instead of inheriting prose.
fn row_style(kind: varde::preview::RowKind, dark: bool) -> Style {
    use varde::preview::RowKind;
    // Prose is not an identifier, so it does not borrow `Kind::Plain`'s blue —
    // it is the editor foreground the code around it is punctuated with.
    let plain = Style::default().fg(match dark {
        true => Color::Rgb(0xd4, 0xd4, 0xd4),
        false => Color::Black,
    });
    match kind {
        RowKind::Heading(level) => heading_style(level, plain),
        RowKind::Paragraph => plain,
        RowKind::Code => Style::default().fg(Color::Cyan),
        RowKind::Diagram => Style::default().fg(Color::Blue),
        RowKind::Metadata | RowKind::Quote(_) | RowKind::Rule => {
            Style::default().fg(Color::DarkGray)
        }
        RowKind::List(_) | RowKind::Table => plain,
    }
}

/// A heading's weight by level. A cell cannot grow a font size, so level is
/// told apart by weight and dimming instead — every level a distinct
/// combination, heaviest at `H1` and plainest at `H6`. Exhaustive on
/// `HeadingLevel` rather than a `_ =>` tail, for the same reason `row_style`
/// itself is: a construct nobody decided the look of is a compile error, not
/// a silent default.
fn heading_style(level: pulldown_cmark::HeadingLevel, plain: Style) -> Style {
    use pulldown_cmark::HeadingLevel;
    let bold = plain.add_modifier(Modifier::BOLD);
    match level {
        HeadingLevel::H1 => bold.add_modifier(Modifier::UNDERLINED),
        HeadingLevel::H2 => bold,
        HeadingLevel::H3 => bold.add_modifier(Modifier::DIM),
        HeadingLevel::H4 => plain.add_modifier(Modifier::UNDERLINED),
        HeadingLevel::H5 => plain.add_modifier(Modifier::DIM),
        HeadingLevel::H6 => plain,
    }
}

/// A row's style with a piece's own modifiers layered on: `code` sets it apart
/// with colour the way a fence already is, `image` reuses the tree's own
/// colour for the same kind of file, and italic, strong and struck are
/// `ratatui`'s modifiers directly rather than a colour, since none of the
/// three is a colour decision.
fn piece_style(kind: varde::preview::RowKind, dark: bool, piece: &varde::preview::Piece) -> Style {
    let emphasis = piece.emphasis;
    let mut style = row_style(kind, dark);
    if emphasis.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if emphasis.strong {
        style = style.add_modifier(Modifier::BOLD);
    }
    if emphasis.struck {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    if emphasis.code {
        style = style.fg(Color::Cyan);
    }
    if emphasis.image {
        style = style.fg(Color::Magenta);
    }
    // Last, because it is the row's colour a token replaces rather than an
    // emphasis: a code row is one flat cyan until the highlighter's answer is
    // read, and `Piece::token` is only ever `Some` inside one.
    if let Some(kind) = piece.token {
        style = style.fg(colour(kind, dark));
    }
    style
}

/// A refusal, in words, for the footer. The reason is `update`'s and the
/// wording is here: a scenario asserts the decision, so rewording this never
/// turns the suite red. Nothing is ever refused in silence — that is the
/// failure `src/keys.rs` is shaped to prevent.
fn refusal_spans(state: &State) -> Vec<Span<'static>> {
    let Some(refusal) = &state.refusal else {
        return Vec::new();
    };
    let wording = match refusal {
        varde::preview::Refusal::NotMarkdown => " not markdown — :preview reads .md ".to_string(),
        varde::preview::Refusal::NoFileOpen => " no file open ".to_string(),
        varde::preview::Refusal::ReadOnlyPreview => " preview — :preview to edit ".to_string(),
        varde::preview::Refusal::GuestReadOnly => " read-only — not your repository ".to_string(),
        varde::preview::Refusal::ToolAlreadyInstalled => " already installed ".to_string(),
        varde::preview::Refusal::BrokenConfig(error) => format!(" {error} "),
        varde::preview::Refusal::NeedsInstaller(installer) => {
            format!(" {installer} is not installed — install.sh installs package managers ")
        }
        varde::preview::Refusal::SessionRunning => {
            " a debug session is already running ".to_string()
        }
        varde::preview::Refusal::NoDebugAdapter(adapter) => {
            format!(" no debug adapter: {adapter} — install it from Tools ")
        }
        varde::preview::Refusal::NoLanguageServer(server) => {
            format!(" the {server} language server hosts this debug adapter — open a {server} file and wait for it ")
        }
        varde::preview::Refusal::DebugAdapterFailed => {
            " the debug adapter could not be started ".to_string()
        }
        varde::preview::Refusal::DebugAdapterExited => " the debug adapter stopped ".to_string(),
        varde::preview::Refusal::LaunchFailed(why) => format!(" could not start: {why} "),
        varde::preview::Refusal::NoLastSession => {
            " nothing to restart — start a launch configuration first ".to_string()
        }
        varde::preview::Refusal::NoRunMark => " nothing on this line to run ".to_string(),
        varde::preview::Refusal::SetValueFailed(why) => format!(" could not set: {why} "),
    };
    vec![Span::styled(wording, Style::default().fg(WARNING))]
}

/// The editor pane's title for an open buffer: its name, whether it holds
/// unsaved work or changed underneath, and what keys will do to it. Shared, so
/// Story view's code surface cannot quietly stop reporting an unsaved buffer —
/// staleness is load-bearing while walking.
///
/// A divergence names the key that resolves it. A marker that only says
/// something is wrong leaves the reader hunting the cheatsheet for the one
/// binding they need at exactly the moment they are about to lose work.
fn buffer_title(
    name: &str,
    buffer: &varde::editor::Buffer,
    mode: &str,
    room: usize,
) -> Line<'static> {
    // The trailing clauses first, so what is left is what the name may have.
    // The name is the one part of the title with no bound, so it is the part
    // that gives: ratatui draws a left-aligned title over a right-aligned one,
    // and a long filename would otherwise cover a Transport control that can
    // then be clicked and not seen.
    let mut trailing = vec![];
    if buffer.is_dirty() {
        trailing.push(Span::styled(" ●", Style::default().fg(DIRTY)));
    }
    // The clause is coloured, never the whole title: a name drawn in orange
    // says the file is alarming, when what is alarming is one thing about it.
    if buffer.changed_on_disk {
        trailing.push(Span::styled(
            "  ⚠ diverged from disk — D to resolve",
            Style::default().fg(WARNING),
        ));
    }
    trailing.push(Span::raw(format!("  [{mode}]")));
    let taken: usize = trailing
        .iter()
        .map(|span| span.content.as_ref().width())
        .sum();
    let mut spans = vec![Span::raw(truncate(name, room.saturating_sub(taken)))];
    spans.extend(trailing);
    Line::from(spans)
}

/// Everything the editor's top border says at its right-hand end: F40's
/// Authorship, then R35.8's Transport — its Chips, a column of border after
/// each, the last of them against the corner. `layout::strip_at` hit-tests
/// exactly those columns, and a test below pins ratatui's placement of a
/// right-aligned title so the two cannot drift; the Authorship goes to the left
/// of them for that reason, and `room` is what the filename left it.
///
/// One `Line` rather than two titles: ratatui lays two right-aligned titles out
/// in an order nothing here would pin, and a clause drawn into the Transport's
/// columns is a control that can be clicked and not seen.
///
/// Nothing at all for a buffer that cannot be read and no Authorship to report:
/// both are empty then, and an empty `Line` draws no title.
fn right_title(state: &State, room: usize, width: u16) -> Line<'static> {
    let mut spans = match authorship_clause(state, room) {
        clause if clause.is_empty() => Vec::new(),
        clause => vec![Span::styled(clause, Style::default().fg(Color::DarkGray))],
    };
    let chips = varde::reading::transport(state);
    let labels = layout::chip_labels(&chips, width, layout::EDITOR_TITLE);
    let gap = Style::default().fg(border_colour(state, Pane::Editor));
    for (chip, label) in chips.iter().zip(labels) {
        spans.extend(chip_spans(state, chip, label));
        spans.push(Span::styled("\u{2500}", gap));
    }
    Line::from(spans).right_aligned()
}

/// The Variables' Transport, on the Strip's top border left of the Group tabs
/// — at the columns `mouse::strip_chip_at` hit-tests, both off
/// `varde::transport_area`. Drawn as its own strip rather than as the pane's
/// title, for the reason the tabs are: the two share one border, and a title
/// flush to the right would sit under them.
fn strip_chips(frame: &mut Frame, state: &State, strip: Area) {
    let chips = varde::debug::strip_transport(state);
    if chips.is_empty() || !varde::showing_transport(state) {
        return;
    }
    let area = varde::transport_area(state, strip);
    let labels = layout::chip_labels(&chips, area.width, layout::CORNER_TITLE);
    let Some(mut x) = area.right().checked_sub(layout::strip_width(&labels)) else {
        return;
    };
    for (chip, label) in chips.iter().zip(labels) {
        let columns = label.width() as u16;
        frame.render_widget(
            Line::from(chip_spans(state, chip, label).to_vec()),
            Rect::new(x, strip.y, columns, 1),
        );
        x += columns;
    }
}

fn group_tabs(frame: &mut Frame, state: &State, strip: Area) {
    let tabs = varde::group_tabs(state);
    let labels = varde::group_labels(state);
    let width = layout::strip_width(&labels);
    let Some(mut x) = strip.right().saturating_sub(1).checked_sub(width) else {
        return;
    };
    // Each label on its own, so the column of border after it is left as the
    // split drew it, in the split's own colour. A tab holding output nobody
    // has read is bold and in the notice colour — the Chip's own mark, one
    // border over, so the two say the same thing the same way.
    for (tab, label) in tabs.iter().zip(labels) {
        let style = match (tab.lit, tab.unseen) {
            (true, false) => Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::REVERSED),
            (true, true) => Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::REVERSED | Modifier::BOLD),
            (false, true) => Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            (false, false) => Style::default().fg(Color::DarkGray),
        };
        let columns = label.width() as u16;
        frame.render_widget(
            Span::styled(label, style),
            Rect::new(x, strip.y, columns, 1),
        );
        x += columns + 1;
    }
}

/// One Chip as drawn: its glyph in its hue and its keys dimmer, dimmed whole
/// while it cannot act, and reversed whole while it is the last action taken.
/// Every colour is one of the terminal's named ones, so the Chip belongs to
/// whatever theme it is drawn in (ADR 0022). Under the pointer it is bold and
/// underlined as well, for the reason a row's action icons light up there: a
/// terminal has no hand pointer to say a thing is a button.
fn chip_spans(state: &State, chip: &varde::Chip, label: String) -> [Span<'static>; 2] {
    let hue = match chip.hue {
        varde::Hue::Go => Color::Green,
        varde::Hue::Hold => Color::Yellow,
        varde::Hue::Step => Color::Blue,
        varde::Hue::Halt => Color::Red,
        varde::Hue::Plain => Color::Reset,
    };
    let (glyph, keys, mut lift) = match chip.tone {
        varde::Tone::Dimmed => (Color::DarkGray, Color::DarkGray, Modifier::empty()),
        varde::Tone::Plain => (hue, Color::DarkGray, Modifier::empty()),
        varde::Tone::Lit => (hue, hue, Modifier::REVERSED),
        // Bold and in the notice colour rather than reversed: reversed is what
        // says "you just pressed this", and nobody pressed this one.
        varde::Tone::Marked => (Color::Yellow, Color::DarkGray, Modifier::BOLD),
    };
    if state.hovered_action == Some(chip.action) {
        lift |= Modifier::BOLD | Modifier::UNDERLINED;
    }
    // The label opens with the glyph and a column each side of it; what is
    // left is the keys, or nothing once the Transport has shed them.
    let (head, tail) = label.split_at(chip.glyph.len() + 2);
    [
        Span::styled(
            head.to_string(),
            Style::default().fg(glyph).add_modifier(lift),
        ),
        Span::styled(
            tail.to_string(),
            Style::default().fg(keys).add_modifier(lift),
        ),
    ]
}

/// A row's Chip, which is its glyph alone: a row has no room for the keys a
/// Transport's Chip carries, and the row's keys are in the cheatsheet. Dimmed
/// says the action cannot run here, and the arrows stepping onto it or the
/// pointer resting on it lift it, exactly as a Transport's does.
fn chip_style(state: &State, chip: &varde::Chip, armed: bool) -> Style {
    if armed || state.hovered_action == Some(chip.action) {
        return Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
    }
    match chip.tone {
        varde::Tone::Dimmed => Style::default().fg(Color::DarkGray),
        _ => Style::default().fg(match chip.hue {
            varde::Hue::Go => Color::Green,
            varde::Hue::Hold => Color::Yellow,
            varde::Hue::Step => Color::Blue,
            varde::Hue::Halt => Color::Red,
            varde::Hue::Plain => Color::Reset,
        }),
    }
}

/// F40. Who last committed the line the cursor is on, and the day they wrote
/// it, drawn dimmed on the top border to the left of the Transport — or that
/// nobody has committed it yet. Empty whenever
/// [`varde::authorship::at_cursor`] has nothing to say.
///
/// The name is what gives when the border runs out of room: the date is ten
/// columns whatever the commit, and a name cut short still says which hand,
/// while a date cut short says the wrong day.
fn authorship_clause(state: &State, room: usize) -> String {
    let Some(authorship) = varde::authorship::at_cursor(state) else {
        return String::new();
    };
    let (who, when) = match &authorship {
        varde::authorship::Authorship::Committed(authored) => {
            (authored.author.as_str(), format!("  {}", authored.date))
        }
        varde::authorship::Authorship::NotCommittedYet => ("Not committed yet", String::new()),
    };
    // One space each side, so the clause is not against a corner or against the
    // first control — the reason the Transport keeps a column of air too.
    let left = room.saturating_sub(2 + when.width());
    match left {
        0 => String::new(),
        left => format!(" {}{when} ", truncate(who, left)),
    }
}

/// What the border leaves the left-hand title: the width inside its two
/// corners, less the Transport at the far end. The Authorship takes no share of
/// it — the name of the file this pane is showing is the last thing on the
/// border to give, and a pane too narrow for both says nothing about who wrote
/// the line rather than nothing about which file it is in.
fn title_room(state: &State, width: u16) -> usize {
    let labels = layout::chip_labels(
        &varde::reading::transport(state),
        width,
        layout::EDITOR_TITLE,
    );
    // A column of border between the two, when there is a Transport at all: a
    // name cut to land exactly against a Chip reads as one word with it.
    let strip = match labels.is_empty() {
        true => 0,
        false => layout::strip_width(&labels) as usize + 1,
    };
    (width as usize).saturating_sub(2 + strip)
}

/// A charwise drag, cut at the columns rather than styling the whole row.
/// Shared by all three text surfaces: `mouse` never asks whether a Story is
/// being walked, so a drag in Story view sets a selection either way, and one
/// that draws nothing reads as `C-c` being broken — which is exactly how a
/// Preview's selection read, drawn nowhere at all.
///
/// The span is the caller's to name, because a Preview's is a `Screen` one
/// over rendered rows while Source's is a `Buffer` one over lines (ADR-0007):
/// read from the wrong shape it is a highlight over characters nobody picked.
/// `has_gutter` is what the surface draws in front of its text, the same fact
/// `layout::gutter` answers for the caret and the hit-test, and `row` is where
/// a line is drawn among `lines` — nowhere, for a line outside the editor's
/// window.
fn paint_drag(
    lines: &mut [Line<'static>],
    row: &dyn Fn(usize) -> Option<usize>,
    span: Option<(varde::Place, varde::Place)>,
    occurrences: &[Place],
    has_gutter: bool,
) {
    let Some((from, to)) = span else {
        return;
    };
    // The occurrences the next-occurrence gesture has taken, each as long as
    // the span above — drawn the same way, because a picked word nobody can
    // see is a keystroke that lands somewhere the reader was not looking. Only
    // while there is a width to draw: once typing has begun they are insertion
    // points, and a cursor of no width is nothing to paint. A span crossing
    // rows is a passage rather than a word, so it names no width and takes the
    // early way out.
    let width = if from.line == to.line {
        to.column - from.column
    } else {
        return paint_span(lines, row, from, to, has_gutter);
    };
    for start in occurrences {
        paint_span(
            lines,
            row,
            *start,
            Place {
                line: start.line,
                column: start.column + width,
            },
            has_gutter,
        );
    }
    paint_span(lines, row, from, to, has_gutter)
}

/// One charwise span, cut at the columns rather than styling the whole row.
fn paint_span(
    lines: &mut [Line<'static>],
    row: &dyn Fn(usize) -> Option<usize>,
    from: Place,
    to: Place,
    has_gutter: bool,
) {
    for number in from.line..=to.line {
        let Some(line) = row(number).and_then(|index| lines.get_mut(index)) else {
            continue;
        };
        let first = if number == from.line {
            from.column - 1
        } else {
            0
        };
        let last = if number == to.line {
            to.column
        } else {
            usize::MAX
        };
        *line = picked(
            line,
            first,
            last,
            Style::default().add_modifier(Modifier::REVERSED),
            has_gutter,
        );
    }
}

/// Slides every row left to the column `editor_hscroll` names, so the tail of a
/// long line is on screen instead of under the pane next door. The first
/// `chrome` spans are not text and do not move — that is what keeps the line
/// numbers readable while the code slides past them, and what keeps a diff's
/// `+`/`-` on the row it names. A surface says how many it has rather than
/// this counting one: Source and Story view's code number their rows, a diff
/// numbers and marks them, and a Preview draws neither.
///
/// Last, after the drag and the search marks: those count columns from the
/// start of the line, and a line already shifted would have them land somewhere
/// else. Before the mark, for the reason `paint_drag` is — a barred line's
/// gutter is two spans, not one.
fn shift(lines: &mut [Line<'static>], state: &State, chrome: usize) {
    // Not just an early exit: shifting by nothing still rebuilds every span of
    // every visible line, on every frame, in the case that is almost always the
    // one — and a frame already costs milliseconds (`pump`).
    if state.editor_hscroll == 0 {
        return;
    }
    for line in lines.iter_mut() {
        let mut spans = Vec::new();
        let mut column = 0;
        for (index, span) in line.spans.iter().enumerate() {
            if index < chrome {
                spans.push(span.clone());
                continue;
            }
            let width = span.content.chars().count();
            if column + width > state.editor_hscroll {
                let kept = span
                    .content
                    .chars()
                    .skip(state.editor_hscroll.saturating_sub(column))
                    .collect::<String>();
                spans.push(Span::styled(kept, span.style));
            }
            column += width;
        }
        // The line's own style, not only its spans': a visual selection lives
        // there, and dropping it makes the selection vanish once you scroll.
        *line = Line::from(spans).style(line.style);
    }
}

/// One line's tokens, each in its kind's colour. The one place a `Kind`
/// becomes a `Span`, so the editor's rows, the diff's rows and a search hit
/// cannot come to spell the same token three ways.
fn spans(tokens: &[highlight::Token], dark: bool) -> Vec<Span<'static>> {
    tokens
        .iter()
        .map(|token| {
            Span::styled(
                token.text.clone(),
                Style::default().fg(colour(token.kind, dark)),
            )
        })
        .collect()
}

/// The buffer's text as numbered, syntax-coloured rows. Shared by Edit view
/// and Story view's code surface, so the two cannot disagree about what a line
/// of code looks like — only about what is drawn over it.
/// The buffer line the caret is on, for the gutter to draw brighter. Nothing
/// for a surface that has no caret in the text — a diff and a Preview number
/// rows rather than lines, and a bright number on one of those would point at
/// a line nobody is on.
fn cursor_line(state: &State) -> Option<usize> {
    match state.diff.is_some() || varde::previewing(state) {
        true => None,
        false => varde::current_buffer(state).map(|buffer| buffer.line),
    }
}

fn code_lines(
    tokens: &[Vec<highlight::Token>],
    numbers: impl IntoIterator<Item = usize>,
    dark: bool,
    cursor: Option<usize>,
) -> Vec<Line<'static>> {
    numbers
        .into_iter()
        .map(|number| {
            let mut row = numbered(number, cursor);
            for span in spans(tokens.get(number - 1).map_or(&[], Vec::as_slice), dark) {
                row.push_span(span);
            }
            row
        })
        .collect()
}

/// Story view's code surface, drawn into the editor pane's own rectangle the
/// way Review's diff already substitutes itself there. Not a fifth `Pane`:
/// input routing, focus and the mouse hit-test are unchanged, and a fourth
/// rectangle would ripple through the layout and every exhaustive match for no
/// behaviour anyone asked for.
///
/// The Site's lines keep their syntax colour and carry a bar in the gutter;
/// every other line is flattened to one grey. Colour itself is the signal —
/// the marked lines end up the only place on screen with any, which is a cue
/// you catch without looking for it. Nothing here decides *which* lines: it
/// asks [`story::mark`] once and derives the rest, because a line is dimmed
/// exactly when it is outside the mark.
///
/// Comments sit under the lines they cover, out of the same store and in the
/// same shape Review's diff draws them: walking is a comment-as-you-read
/// surface, and one that cannot show you what you already said is one you say
/// it to twice.
/// The Step's file with its Site barred and everything outside it dimmed, and
/// the reviewer's comments interleaved.
fn marked_code(
    state: &State,
    tokens: &[Vec<highlight::Token>],
    marked: &story::SiteMark,
    relative: &str,
) -> Vec<Line<'static>> {
    let mut code = code_lines(
        tokens,
        1..=tokens.len(),
        state.editor_theme != "light",
        cursor_line(state),
    );
    // Before the mark, so a dragged span is already on the line the mark then
    // bars or dims — `picked` counts the gutter as one span, which a barred
    // line no longer is.
    paint_drag(
        &mut code,
        &|number| Some(number - 1),
        state.selection.as_ref().and_then(Selection::buffer_span),
        &state.occurrences,
        true,
    );
    shift(&mut code, state, 1);
    let dark = state.editor_theme != "light";
    let diff = story::site_diff(state);
    // Only a mark divides the file into inside and outside — or `d` over an
    // old-side Site, whose lines are all outside: they are the removed rows.
    let site = match marked {
        story::SiteMark::Site { kind, .. } => Some(bar(*kind)),
        _ => None,
    };
    if site.is_some() || diff.is_some() {
        for (index, line) in code.iter_mut().enumerate() {
            let number = index + 1;
            let added = diff
                .as_ref()
                .is_some_and(|diff| diff.added.contains(&(number as u32)));
            *line = if added {
                changed_row(line, Some(number), false, dark)
            } else if let Some(colour) = site.filter(|_| marked.covers(relative, number as u32)) {
                barred(line, number, colour)
            } else {
                dimmed(line)
            };
        }
    }
    // Comments last, so the mark above was asked about *lines* while a line's
    // number was still its position. `story::rows` carries the number across
    // the interleave; marking afterwards against a row index would slide every
    // bar below a comment down by one.
    story::rows(state, code.len())
        .map(|row| match row {
            // Taken, not cloned: `rows` yields each line exactly once, so the
            // one left behind is never read again.
            story::Row::Code(number) => std::mem::take(&mut code[number as usize - 1]),
            story::Row::Comment(comment) => comment_row(&comment.kind, &comment.body),
            // Slid with the code, since it is code: a removed line the view had
            // slid past would otherwise start at a column nothing else does.
            story::Row::Removed(text) => {
                let mut gone = [Line::from(vec![Span::raw(""), Span::raw(text)])];
                shift(&mut gone, state, 1);
                changed_row(&gone[0], None, true, dark)
            }
        })
        .collect()
}

fn story_widget(
    state: &State,
    command: Option<&str>,
    tokens: &[Vec<highlight::Token>],
    width: u16,
) -> Paragraph<'static> {
    let marked = story::mark(state);
    let name = state
        .current_buffer
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let relative = story::shown_file(state);
    let refused = story::refused(state);
    let lines: Vec<Line> = if refused {
        // The Site's own file, not the buffer's: the buffer holds the working
        // tree's version of it and need not be open at all, and what the notice
        // has to name is the file the claim describes.
        refusal(
            story::current_step(state)
                .map(|step| step.site.file.as_str())
                .unwrap_or(relative.as_str()),
        )
    } else {
        marked_code(state, tokens, &marked, &relative)
    };
    // No `pending_command`: while walking, every key is intercepted before it
    // reaches the buffer, so there is never a chord half-typed to report.
    let mut footer = dot_spans(state);
    // No position on a refusal: the cursor is in the working-tree file, and a
    // line number reported for text nobody can see names a place in the wrong
    // code — the same lie in miniature as the mark this Step refuses to draw.
    if let Some(buffer) = state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path))
        .filter(|_| !refused)
    {
        footer.push(Span::styled(
            varde::mouse::position_label(buffer.line, buffer.column),
            Style::default().fg(Color::Cyan),
        ));
    }
    let title = match state
        .current_buffer
        .as_ref()
        .and_then(|path| state.buffers.get(path))
    {
        Some(buffer) => buffer_title(
            &name,
            buffer,
            varde::mode_label(state, buffer),
            title_room(state, width),
        ),
        None => name.into(),
    };
    Paragraph::new(lines)
        .scroll((state.editor_scroll as u16, 0))
        .block(editor_block(state, title, footer, command, width))
}

/// What an old-side Site shows instead of code. `OpenAt` is executed here as a
/// read of the working tree, so the buffer behind this pane holds the *new*
/// text at old line numbers: drawing it — plain, dimmed or barred — puts code
/// on screen that is not the code the claim describes, which is the one thing
/// in this feature that was wrong rather than merely missing. The Step is still
/// a Step, so it says where the claim it cannot illustrate has gone.
///
/// It promises nothing it cannot show. An earlier draft pointed at `c`, which
/// does still open a comment — but a comment made here is drawn nowhere, since
/// the refusal replaces the rows `story::rows` would have interleaved it into.
///
/// The path goes into a `Span`, never a raw write: a filename is untrusted text
/// and a ratatui span never interprets an escape one might carry.
fn refusal(file: &str) -> Vec<Line<'static>> {
    vec![
        Line::from(""),
        Line::styled(
            format!("  The old text of {file} cannot be shown."),
            Style::default().fg(Color::Yellow),
        ),
        Line::styled(
            "  This step points at the base revision.",
            Style::default().fg(Color::DarkGray),
        ),
        Line::from(""),
        Line::styled(
            "  Its claim is in the band below.",
            Style::default().fg(Color::DarkGray),
        ),
    ]
}

/// A marked line's gutter: the number, then a bar in the Site's own colour.
/// The bar takes the blank column the number was already padded with rather
/// than adding one — `layout::GUTTER` is where text starts, and the mouse
/// hit-test and the caret both count on it.
fn barred(line: &Line<'static>, number: usize, colour: Color) -> Line<'static> {
    let mut spans = vec![
        // Brighter than the grey `dimmed` flattens to: the Site's own numbers
        // are the ones worth reading, and two identical greys say nothing.
        Span::styled(format!(" {number:>4}"), Style::default().fg(Color::Gray)),
        Span::styled("▌", Style::default().fg(colour)),
        // The pad columns the bar did not take, one of which `with_toggle`
        // then claims — which is what the seventh gutter column buys: news and
        // affordance no longer compete for the one column there was.
        Span::raw(PAD),
    ];
    spans.extend(line.spans.iter().skip(1).cloned());
    Line::from(spans)
}

/// The gutter's pad, past the four columns of line number and the fifth a bar
/// takes: the fold toggle sits in the first of these and the other two are the
/// air between the gutter and the code. Also the shape [`with_toggle`]
/// recognises a barred line by.
const PAD: &str = "   ";

/// Everything outside the mark, flattened to one grey — the line number with
/// it, so the Site's own numbers stay the legible ones.
fn dimmed(line: &Line<'static>) -> Line<'static> {
    Line::from(
        line.spans
            .iter()
            .map(|span| Span::styled(span.content.clone(), span.style.fg(Color::DarkGray)))
            .collect::<Vec<_>>(),
    )
}

/// Which kind of Site the bar is announcing, so that "is this part of the
/// change" lives inside the mark rather than in a second system the reviewer
/// has to consult. Two colours, and no scenario asserts either.
fn bar(kind: story::Kind) -> Color {
    match kind {
        story::Kind::Changed => Color::Yellow,
        story::Kind::Context => Color::Blue,
    }
}

/// A reminder of the keys, tucked into the editor's top-right. `:help` takes it
/// down and puts it back — a terminal cell holds one character, so while it is
/// up the code under it is gone and there is no opacity to give it. Hidden while
/// Edit view is inserting, when you are typing rather than remembering, and
/// hidden in Review view until a diff has actually landed — Review's rows
/// answer to `state.diff`, and one that has not shown up yet claims nothing.
/// There is no third arm for Edit view with a diff on screen:
/// `move_to_view` clears `state.diff` on every
/// way out of Review, so the two never coexist. The keys themselves live in
/// `keys::CHEATSHEET`, one row per gesture tagged with the views it applies
/// to; drawing only filters the table to `state.view` rather than deciding
/// what belongs in it.
/// Whether the view has keys to claim at all.
fn showing_cheatsheet(state: &State) -> bool {
    match state.view {
        View::Edit => state
            .current_buffer
            .as_ref()
            .and_then(|path| state.buffers.get(path))
            .is_some_and(|buffer| buffer.mode != Mode::Insert),
        // Review's rows only answer once a diff is on screen — `j k V c` and
        // `e` are read from `state.diff` in `update`, and an empty review or
        // one whose diff has not landed yet has none of them to claim.
        View::Review => state.diff.is_some(),
        // Nothing gates it: Story view has no content yet whose absence
        // should hide the box, unlike Edit's insert mode or Review's diff.
        View::Story => true,
    }
}

/// The rows the box actually shows in a pane `height` rows tall: the table
/// filtered to the view, and then cut to what
/// there is room for — the pane's height less the border the box starts under
/// and the one it stops above.
///
/// The cut is made here rather than left to `Paragraph`, which drops the
/// surplus without a word. The box has no footer and nowhere to put a mark, so
/// this cannot be announced the way `lsp::TALLEST` announces one; what it can
/// be is *read*, which is what lets a test hold `keys::CHEATSHEET`'s order to
/// the promise its own doc makes — the rows that survive a short window are the
/// ones nothing else teaches you. Twenty-five Edit rows compete for sixteen on
/// a 26-row screen, so the order is the whole of the answer.
fn cheatsheet_rows(state: &State, height: u16) -> Vec<(String, Color)> {
    let rows_for_view: Vec<(&str, &str)> = keys::cheatsheet(state)
        .filter(|(_, _, views)| state.cheatsheet && keys::applies_to(views, state.view))
        .map(|(keys, what, _)| (*keys, *what))
        .collect();
    let column = rows_for_view
        .iter()
        .map(|(keys, _)| keys.len())
        .max()
        .unwrap_or(0);
    let mut rows: Vec<(String, Color)> = rows_for_view
        .into_iter()
        .map(|(keys, what)| (format!(" {keys:column$}  {what}"), Color::DarkGray))
        .collect();
    rows.truncate(height.saturating_sub(2) as usize);
    rows
}

/// The replace box, while it has the keyboard: find, with, and the two
/// buttons, at the rows and columns `mouse` hit-tests them by. `[Aa]` is the
/// search's own toggle, drawn a second time rather than being a second
/// setting. Its keys go in the bottom border, for the reason the comment box's
/// do.
fn replace_box(frame: &mut Frame, state: &State, editor: Area) {
    let Some(find) = state.find.as_ref() else {
        return;
    };
    let varde::FindKeys::Replace(field) = find.keys else {
        return;
    };
    let spot = layout::replace_box(editor);
    let on = |at| match at == field {
        true => Style::default().add_modifier(Modifier::REVERSED),
        false => Style::default(),
    };
    let text = |buffer: &varde::editor::Buffer, at| match at == field {
        true => with_caret(buffer.shown(), buffer.column),
        false => vec![Span::raw(buffer.shown().to_string())],
    };
    let mut find_row = vec![Span::raw(" find  ")];
    find_row.extend(text(&find.query, varde::ReplaceField::Find));
    let mut with_row = vec![Span::raw(" with  ")];
    with_row.extend(text(&state.replace_with, varde::ReplaceField::With));
    let footer = keys::REPLACE_BOX_KEYS
        .iter()
        .map(|(key, word)| format!("{key} {word}"))
        .collect::<Vec<_>>()
        .join(" · ");
    let area = rect(spot);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![Line::from(find_row), Line::from(with_row)]).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" replace ")
                .title_bottom(Line::from(Span::styled(
                    format!(" {footer} "),
                    Style::default().fg(Color::DarkGray),
                ))),
        ),
        area,
    );
    frame.render_widget(
        Span::styled(
            varde::FIND_ICONS[0].1,
            match find.case.exact(find.query.shown()) {
                true => Style::default().fg(Color::Yellow),
                false => Style::default().fg(Color::DarkGray),
            },
        ),
        rect(layout::replace_case(spot)),
    );
    for ((at, label), (_, spot)) in varde::REPLACE_BUTTONS
        .iter()
        .zip(layout::replace_buttons(spot))
    {
        frame.render_widget(Span::styled(*label, on(*at)), rect(spot));
    }
}

fn cheatsheet(frame: &mut Frame, state: &State, area: Rect) {
    if !showing_cheatsheet(state) || state.focus != Pane::Editor {
        return;
    }
    let rows = cheatsheet_rows(state, area.height);
    let width = rows.iter().map(|(row, _)| row.len()).max().unwrap_or(0) as u16 + 1;
    if area.width < width + 12 {
        return;
    }
    let spot = Rect {
        x: area.right().saturating_sub(width + 1),
        y: area.y + 1,
        width,
        height: rows.len() as u16,
    };
    frame.render_widget(Clear, spot);
    frame.render_widget(
        Paragraph::new(
            rows.into_iter()
                .map(|(row, color)| Line::from(Span::styled(row, Style::default().fg(color))))
                .collect::<Vec<_>>(),
        ),
        spot,
    );
}

/// The hover box, over the lines the core placed it on. The markdown is
/// already read, wrapped and counted — a box sized here from unwrapped text
/// would be drawn one row tall while the message needs three, which is why
/// neither that arithmetic nor the parse behind it is the renderer's. All that is left is turning a buffer line
/// into a screen row, off the same `editor_scroll` the code lines are drawn
/// from, and keeping the box inside the pane.
fn hover(frame: &mut Frame, state: &State, panes: &layout::Layout) {
    let (Some(hover), Some(placement)) = (state.hover.as_ref(), lsp::placement(state)) else {
        return;
    };
    let dark = state.editor_theme != "light";
    // The box's inside: `lsp::measured` adds the two columns its border sits
    // on, so a rule drawn at the full placement would run through them.
    let columns = placement.width.saturating_sub(2);
    let lines = lsp::sections(state)
        .iter()
        .skip(hover.first)
        .map(|said| match said {
            // What the program holds, above what the server says it is: the
            // reader stopped in their program reads this first.
            varde::lsp::Said::Value(row) => Line::from(vec![
                Span::styled(
                    format!(
                        "{}{}{} ",
                        " ".repeat(row.depth * 2),
                        match (row.opens, row.open) {
                            (varde::debug::Opens::Nothing, _) => "",
                            (_, true) => "\u{25be} ",
                            (_, false) => "\u{25b8} ",
                        },
                        row.name
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(row.value.clone()),
            ]),
            // Said out loud rather than left blank: a box with nothing where
            // the value goes reads as a debugger that failed, and this one
            // declined on purpose.
            varde::lsp::Said::Needs => Line::from(Span::styled(
                varde::lsp::NEEDS_EVALUATE,
                Style::default().fg(Color::Yellow),
            )),
            varde::lsp::Said::Docs(row) => preview_line(row, dark, columns),
        })
        .collect();
    over_buffer_line(frame, state, panes, placement, lines);
    hover_chips(frame, state, placement.spot(state, panes));
}

fn breakpoint_reason(frame: &mut Frame, state: &State, panes: &layout::Layout) {
    let Some((line, why)) = varde::debug::explained(state) else {
        return;
    };
    let placement = varde::lsp::Placement {
        from: line + 1,
        column: 1,
        width: why.width() + 2,
        rows: 3,
    };
    over_buffer_line(
        frame,
        state,
        panes,
        placement,
        vec![Line::from(why.to_string())],
    );
}

/// The Evaluator: the Snippet above, the output below, and the Chips on the
/// top border. Every rectangle here is `layout`'s — the mouse hit-tests the
/// same ones — and every row of the output is `debug`'s, so this only draws.
fn evaluator(frame: &mut Frame, state: &State, panes: &layout::Layout) {
    let Some(open) = state.evaluator.as_ref() else {
        return;
    };
    let window = panes.evaluator;
    let (snippet_area, output_area) = layout::evaluator_split(window, open.snippet_rows);
    frame.render_widget(Clear, rect(window));
    frame.render_widget(
        Block::default().borders(Borders::ALL).title("EVALUATE"),
        rect(window),
    );
    let lines: Vec<Line> = open
        .snippet
        .shown()
        .split('\n')
        .map(|row| Line::raw(row.to_string()))
        .collect();
    frame.render_widget(Paragraph::new(lines), rect(snippet_area));
    // The rule between the two, on the row `layout` left for it.
    frame.render_widget(
        Block::default().borders(Borders::TOP),
        rect(varde::layout::Area {
            height: 1,
            y: output_area.y.saturating_sub(1),
            ..output_area
        }),
    );
    let output: Vec<Line> = varde::debug::evaluator_output(state)
        .iter()
        .map(|line| match line {
            varde::debug::Said::Printed(text) => Line::from(Span::styled(
                text.clone(),
                Style::default().fg(Color::DarkGray),
            )),
            varde::debug::Said::Running => Line::from(Span::styled(
                "running\u{2026}",
                Style::default().fg(Color::Yellow),
            )),
            // The adapter's words as it said them: a compile error the reader
            // cannot read is a Snippet they cannot fix.
            varde::debug::Said::Failed(why) => Line::from(Span::styled(
                why.clone(),
                Style::default().fg(Color::LightRed),
            )),
            varde::debug::Said::Value(row) => Line::from(vec![
                Span::styled(
                    format!(
                        "{}{}{} ",
                        " ".repeat(row.depth * 2),
                        match (row.opens, row.open) {
                            (varde::debug::Opens::Nothing, _) => "",
                            (_, true) => "\u{25be} ",
                            (_, false) => "\u{25b8} ",
                        },
                        row.name
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(row.value.clone()),
            ]),
        })
        .collect();
    frame.render_widget(Paragraph::new(output), rect(output_area));
    evaluator_chips(frame, state, window);
    if state.focus == varde::Pane::Evaluator {
        if let Some(buffer) = state.edited() {
            frame.set_cursor_position((
                snippet_area.x + buffer.column.saturating_sub(1) as u16,
                snippet_area.y + buffer.line.saturating_sub(1) as u16,
            ));
        }
    }
}

/// The Evaluator's Chips along its top border, at the columns
/// `mouse::pressed_in_evaluator` hit-tests them at — flush right, as every
/// other strip of Chips on a border is.
fn evaluator_chips(frame: &mut Frame, state: &State, window: varde::layout::Area) {
    let chips = varde::debug::evaluator_chips(state);
    let labels = varde::debug::evaluator_labels(state, window.width);
    let Some(mut x) = window.right().checked_sub(layout::strip_width(&labels)) else {
        return;
    };
    for (chip, label) in chips.iter().zip(labels) {
        let width = label.width() as u16;
        frame.render_widget(
            Line::from(chip_spans(state, chip, label).to_vec()),
            Rect::new(x, window.y, width, 1),
        );
        x += width;
    }
}

/// The Hover's Chips along its top border, at the columns
/// `mouse::pressed_in_hover` hit-tests them at — flush right, as every other
/// strip of Chips on a border is.
fn hover_chips(frame: &mut Frame, state: &State, spot: varde::layout::Area) {
    let chips = varde::debug::hover_chips(state);
    let labels = varde::debug::hover_labels(state, spot.width);
    let Some(mut x) = spot.right().checked_sub(layout::strip_width(&labels)) else {
        return;
    };
    for (chip, label) in chips.iter().zip(labels) {
        let width = label.width() as u16;
        frame.render_widget(
            Line::from(chip_spans(state, chip, label).to_vec()),
            Rect::new(x, spot.y, width, 1),
        );
        x += width;
    }
}

/// What is wrong with the characters the pointer rests on, beside the line they
/// are on. The message is wrapped and the box is placed by the core, for the
/// reason the hover box is: a box sized here would be one row tall while the
/// message needs three.
fn diagnostic_box(frame: &mut Frame, state: &State, panes: &layout::Layout) {
    let Some((lines, placement)) = lsp::pointed(state) else {
        return;
    };
    let lines = lines
        .into_iter()
        .map(|row| Line::from(Span::styled(row, Style::default().fg(Color::LightRed))))
        .collect();
    over_buffer_line(frame, state, panes, placement, lines);
}

/// The candidate list, over the lines the core placed it on and never over the
/// one being typed. The chosen row is drawn reversed: a list with nothing
/// marked is a list where Enter takes something the reader did not pick.
fn candidates(frame: &mut Frame, state: &State, panes: &layout::Layout) {
    let Modal::Candidates(list) = &state.modal else {
        return;
    };
    let lines = list
        .shown()
        .into_iter()
        .enumerate()
        .skip(list.first)
        .take(list.rows().saturating_sub(2))
        .map(|(at, candidate)| match at == list.selected {
            true => Line::from(candidate.label.clone())
                .style(Style::default().add_modifier(Modifier::REVERSED)),
            false => Line::from(candidate.label.clone()),
        })
        .collect();
    over_buffer_line(frame, state, panes, list.placement(), lines);
}

/// A bordered box over a buffer line, where and as big as the core said. Every
/// number in a `Placement` is the core's, and so is the rectangle it turns into
/// on screen — the mouse hit-tests the same one. One function for every box for
/// the same reason there is one layout.
fn over_buffer_line(
    frame: &mut Frame,
    state: &State,
    panes: &layout::Layout,
    placement: varde::lsp::Placement,
    lines: Vec<Line>,
) {
    let spot = rect(placement.spot(state, panes));
    frame.render_widget(Clear, spot);
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL)),
        spot,
    );
}

/// What the editor's bottom-left line is showing, prefix and all: a `:` command
/// being typed, or the `/` search of this file that is on. One line, two
/// prefixes — which is what keeps finding here looking different from finding
/// everywhere. The search's pieces are `varde::find_line`'s, which the mouse
/// hit-tests too: `[Aa]` dim unless the search is exact, and the icon holding
/// the keyboard reversed.
fn command_line(state: &State, command: Option<&str>) -> Line<'static> {
    let yellow = Style::default().fg(Color::Yellow);
    if let Some(line) = command {
        return Line::from(Span::styled(format!(" {line} "), yellow));
    }
    let Some(find) = state.find.as_ref() else {
        return Line::default();
    };
    let lit = find.case.exact(find.query.shown());
    let pieces = varde::find_line(state).into_iter().map(|(text, icon)| {
        let style = match icon {
            Some(varde::FindIcon::Case) if !lit => Style::default().fg(Color::DarkGray),
            _ => yellow,
        };
        match (icon, find.keys) {
            (Some(icon), varde::FindKeys::Icon(on)) if icon == on => {
                Span::styled(text, style.add_modifier(Modifier::REVERSED))
            }
            _ => Span::styled(text, style),
        }
    });
    Line::from(pieces.collect::<Vec<_>>())
}

/// A search query either side of its caret.
fn at_caret(query: &varde::editor::Buffer) -> (String, String) {
    let text = query.shown();
    let at = text
        .char_indices()
        .nth(query.column - 1)
        .map_or(text.len(), |(at, _)| at);
    (text[..at].to_string(), text[at..].to_string())
}

/// The editor's frame. The `:` line lives on its bottom edge, not at the foot
/// of the screen where the terminal pane is.
fn editor_block(
    state: &State,
    title: Line<'static>,
    footer: Vec<Span<'static>>,
    command: Option<&str>,
    width: u16,
) -> Block<'static> {
    // What the left-hand title did not take, which is what the Authorship may
    // have: ratatui draws a left-aligned title *over* a right-aligned one, so a
    // clause measured against the whole border is a clause drawn under the
    // filename — and `title_room` has already cut the name to the columns the
    // Transport leaves.
    let room = title_room(state, width).saturating_sub(title.width());
    pane_block(title, state, Pane::Editor)
        .title(right_title(state, room, width))
        .title_bottom(Line::from(footer).right_aligned())
        .title_bottom(command_line(state, command).left_aligned())
}

/// The diff's rows, comments and all. Comments anchor to the new-file numbers
/// on the right, which is the code the AI has to change.
///
/// Foreground is spent on the language, out of the same highlighter the editor
/// draws from — reading a change was reading uncoloured code beside a coloured
/// file. So what tells the three kinds of row apart is everything that is *not*
/// the foreground: the marker column keeps its `+` and `-` and its red or
/// green, and the row carries a background tint. A reviewer who cannot tell an
/// addition from context is worse off than one reading grey code, which is the
/// one thing colour here must not cost.
///
/// A row whose side could not be read keeps the flat colour it has always had,
/// its own marker's — an absence drawn as an absence, never filled in from the
/// side that *can* be read.
fn diff_rows(
    state: &State,
    diff: &[varde::DiffLine],
    file: &str,
    new_side: &[Vec<highlight::Token>],
    old_side: &[Vec<highlight::Token>],
) -> Vec<Line<'static>> {
    let dark = state.editor_theme != "light";
    let selected = state.selected_diff_lines();
    let coloured = review::diff_tokens(diff, new_side, old_side);
    let mut rows: Vec<Line> = Vec::new();
    for (index, line) in diff.iter().enumerate() {
        let number = index + 1;
        // A row with a line on both sides is context; one with only a new line
        // is an addition. Both are `removed: false`, which is why the old
        // side's number is what separates them.
        //
        // The dark tints are the one place Varde picks a colour outside the
        // 256-cube: the cube's own dark red and green (52 and 22) are its
        // saturated primaries, and a whole row of code sitting on one is what
        // made a reviewed file unreadable. There is no cube entry between them
        // and the background, so these two are `Rgb` — the only other one in
        // this file passes a child's own colour through. Foregrounds move with
        // them for the same reason: plain red and green are the marker column
        // *and*, when a side could not be highlighted, the whole row's text.
        let (marker, own, tint) = match (line.removed, line.old_line.is_some()) {
            (true, _) => change_colours(true, dark),
            (false, true) => (' ', Color::DarkGray, Color::Reset),
            (false, false) => change_colours(false, dark),
        };
        let mut row = Style::default().bg(tint);
        if selected.is_some_and(|(from, to)| number >= from && number <= to) {
            row = row.add_modifier(Modifier::REVERSED);
        }
        let mut spans = vec![
            Span::styled(
                format!(
                    " {:>4} ",
                    line.new_line.map(|n| n.to_string()).unwrap_or_default()
                ),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(format!("{marker} "), row.fg(own)),
        ];
        match coloured[index] {
            // The language's colours over the row's own tint: patched rather
            // than replaced, so the token keeps the foreground and the row
            // keeps the background and the selection.
            Some(tokens) => spans.extend(
                self::spans(tokens, dark)
                    .into_iter()
                    .map(|span| Span::styled(span.content, span.style.patch(row))),
            ),
            None => spans.push(Span::styled(line.text.clone(), row.fg(own))),
        }
        rows.push(Line::from(spans));
    }
    // The rows before the comments, for the reason `marked_code` slides its
    // code before interleaving them: a comment is about a line, not a column
    // of it, so it stays where it is while the code slides out from under it.
    // Two spans of chrome here rather than Source's one — the number, then the
    // `+`/`-`, which names the row and would be the first thing to slide off it.
    shift(&mut rows, state, 2);
    let mut lines: Vec<Line> = Vec::new();
    for (index, line) in diff.iter().enumerate() {
        // Taken, not cloned: the loop above yields one row per diff line, so
        // the one left behind is never read again — the same move
        // `marked_code` makes over `story::rows`.
        lines.push(std::mem::take(&mut rows[index]));
        // Comments sit under the line they cover, so adding one is visible
        // where you added it.
        for comment in line
            .new_line
            .map(|number| review::comments_at(state, file, number as u32))
            .unwrap_or_default()
        {
            lines.push(comment_row(&comment.kind, &comment.body));
        }
    }
    lines
}

/// A removed or an added row's marker, its own colour and its tint — the one
/// red and green, whether the row is Review's diff or a walked Site's.
fn change_colours(removed: bool, dark: bool) -> (char, Color, Color) {
    match (removed, dark) {
        (true, true) => ('-', Color::Indexed(167), Color::Rgb(0x3a, 0x20, 0x24)),
        (true, false) => ('-', Color::Indexed(167), Color::Indexed(224)),
        (false, true) => ('+', Color::Indexed(71), Color::Rgb(0x1b, 0x35, 0x24)),
        (false, false) => ('+', Color::Indexed(71), Color::Indexed(194)),
    }
}

/// A walked Site's line under `d`: the bar and the text take the change's
/// colours, and the text keeps its language's foreground over the tint, as a
/// diff row does. `removed` has no line number, since it has no line.
fn changed_row(
    line: &Line<'static>,
    number: Option<usize>,
    removed: bool,
    dark: bool,
) -> Line<'static> {
    let (_, own, tint) = change_colours(removed, dark);
    let mut spans = vec![
        Span::styled(
            format!(" {:>4}", number.map(|n| n.to_string()).unwrap_or_default()),
            Style::default().fg(Color::Gray),
        ),
        Span::styled("▌", Style::default().fg(own)),
        Span::raw(PAD),
    ];
    spans.extend(line.spans.iter().skip(1).map(|span| {
        let style = match removed {
            true => span.style.fg(own),
            false => span.style,
        };
        Span::styled(span.content.clone(), style.bg(tint))
    }));
    Line::from(spans)
}

/// A read-only unified diff.
fn diff_widget(
    state: &State,
    diff: &[varde::DiffLine],
    file: &str,
    new_side: &[Vec<highlight::Token>],
    old_side: &[Vec<highlight::Token>],
) -> Paragraph<'static> {
    Paragraph::new(diff_rows(state, diff, file, new_side, old_side))
        .scroll((state.editor_scroll as u16, 0))
        .block(pane_block(
            format!("{file}  [diff — e to edit, V+c to comment]"),
            state,
            Pane::Editor,
        ))
}

/// A comment under the line it covers. One shape for both surfaces: Review's
/// diff and Story view show the same comment out of the same store, and two
/// spellings of it would read as two different things to whoever met both.
fn comment_row(kind: &str, body: &str) -> Line<'static> {
    // A comment is exactly one row on both surfaces: the diff's rows and
    // `story::rows` each count one per comment, and the scroll clamp and the
    // mouse hit-test count with them. So a body's newlines are *shown* rather
    // than obeyed — interpolated raw they are control characters inside a
    // ratatui `Line`, which draws neither a break nor, reliably, the text after
    // it. Giving a comment as many rows as its body has lines is a change to
    // what a row is, which is `story::rows`' to make and not this function's.
    let body = body.lines().collect::<Vec<_>>().join(" ⏎ ");
    Line::from(Span::styled(
        format!("      ▌ {kind} {body}"),
        Style::default().fg(if kind == "ISSUE" {
            Color::Red
        } else {
            Color::Cyan
        }),
    ))
}

/// Puts the terminal's own cursor where the buffer's cursor is. Without this
/// the editor has no visible caret at all.
/// Which row and column of the editor's own text the caret sits on, or nothing
/// where there is no caret to draw.
fn caret_at(state: &State) -> Option<(usize, usize)> {
    match (&state.diff, state.current_buffer.as_ref()) {
        // Already a row rather than a line: Review's diff numbers its own rows,
        // comment rows included.
        (Some(_), _) => Some((state.diff_line, 1)),
        // A Preview's cursor is a row and a **rendered** column — a column of
        // exactly what is drawn, so the caret sits on the character the reader
        // is pointing at rather than at the start of the row.
        (None, Some(path)) if varde::previewing(state) => state
            .buffers
            .get(path)
            .map(|buffer| (buffer.row, buffer.row_column)),
        // Story view draws comment rows between the lines, so the row a line
        // sits on is no longer its number. Edit view has no such rows and
        // `row_of` is the identity there.
        (None, Some(path)) => state
            .buffers
            .get(path)
            .map(|buffer| (story::row_of(state, buffer.line as u32), buffer.column)),
        _ => None,
    }
}

fn place_cursor(frame: &mut Frame, state: &State, areas: &Areas, command: Option<&str>) {
    // While a command is being typed, the caret belongs to the command line.
    if let Some(line) = command {
        frame.set_cursor_position((
            areas.editor.x + 2 + line.chars().count() as u16,
            areas.editor.bottom().saturating_sub(1),
        ));
        return;
    }
    // The candidate list is the one modal the caret survives: it is a list
    // offered while typing, and typing with no caret is typing into a pane
    // that looks like it lost focus.
    let claimed = !matches!(state.modal, Modal::None | Modal::Candidates(_));
    if state.focus != Pane::Editor || claimed {
        return;
    }
    // An old-side Step draws a notice where its code would be, so there is no
    // line for a caret to sit on. Left in, it would point at a row of prose and
    // read as "you are here" in code that is not on screen.
    if story::refused(state) {
        return;
    }
    let Some((line, column)) = caret_at(state) else {
        return;
    };
    // Scrolled past the cursor with the wheel: there is no caret to draw.
    let Some(row) = line.saturating_sub(1).checked_sub(state.editor_scroll) else {
        return;
    };
    // Past the left edge of what the pane is showing sideways: the same case as
    // a row the wheel scrolled off, and answered the same way.
    let Some(column) = column.checked_sub(1 + state.editor_hscroll) else {
        return;
    };
    let x = areas.editor.x + 1 + varde::gutter(state) + column as u16;
    let y = areas.editor.y + 1 + row as u16;
    if x < areas.editor.right().saturating_sub(1) && y < areas.editor.bottom().saturating_sub(1) {
        frame.set_cursor_position((x, y));
    }
}

/// The pty panes have a caret of their own: the child moves it, vt100 tracks
/// where, and ratatui hides the cursor on any frame that does not ask for it —
/// which is why the shell looked caret-less.
fn place_pty_cursor(frame: &mut Frame, area: Rect, pane: &PtyPane) {
    let screen = pane.screen();
    if screen.hide_cursor() || pane.scrolled_back() {
        return;
    }
    let (row, column) = screen.cursor_position();
    let (x, y) = (area.x + 1 + column, area.y + 1 + row);
    if x < area.right().saturating_sub(1) && y < area.bottom().saturating_sub(1) {
        frame.set_cursor_position((x, y));
    }
}

/// One dot per open buffer, filled for the one you are in. A tab bar's
/// information without a tab bar.
fn dot_spans(state: &State) -> Vec<Span<'static>> {
    if state.buffers.len() < 2 {
        return Vec::new();
    }
    let mut spans = Vec::new();
    for path in state.buffers.keys() {
        let (glyph, colour) = glyph(mark(state, path));
        spans.push(Span::styled(glyph, Style::default().fg(colour)));
        spans.push(Span::raw(" "));
    }
    spans
}

/// Filled means it holds something you care about — the one you are in, or one
/// with unsaved work. Colour says which.
fn glyph(mark: Mark) -> (&'static str, Color) {
    match mark {
        Mark::None => (" ", Color::Reset),
        Mark::Open => ("○", Color::DarkGray),
        Mark::Dirty => ("●", DIRTY),
        Mark::Current => ("●", Color::Cyan),
        Mark::CurrentDirty => ("●", DIRTY),
    }
}

/// The line with its indentation guides, its brackets marked and every space
/// drawn as a dot. Which columns hold a guide, which guide is the cursor's and
/// which columns hold the marked brackets are all the library's answer; this
/// only draws them. One pass rather than a rebuild per mark, because the editor
/// redraws every line on screen on every keystroke.
///
/// Every line goes through it, guides or none: a line at no indentation still
/// holds the spaces between its words, and skipping it left the outermost
/// lines of a file the only ones without dots.
fn guided(
    line: &Line<'static>,
    guides: &[varde::editor::Guide],
    brackets: &[usize],
    mark: Style,
    faint: Style,
) -> Line<'static> {
    // Straight either way, and the cursor's block heavier rather than solid: a
    // guide is a fact about the text, so the one the cursor is in is the same
    // line drawn with more weight, not a different kind of line. Heavy box
    // rather than the BOLD attribute, which a terminal is free to ignore on a
    // glyph like this one. The weight is the whole signal and the colour does
    // not move with it: a bright guide reads as a bar down the page rather than
    // as a thicker line, which is louder than anything a guide is worth.
    let glyph = |guide: &varde::editor::Guide| {
        let character = match guide.active {
            true => "\u{2503}",
            false => "\u{2502}",
        };
        Span::styled(character.to_string(), faint)
    };
    let mut spans = Vec::new();
    let mut column = 0;
    for (index, span) in line.spans.iter().enumerate() {
        // The line number is not part of the text the columns are counted in.
        if index == 0 {
            spans.push(span.clone());
            continue;
        }
        for character in span.content.chars() {
            match guides.iter().find(|guide| guide.column == column) {
                Some(guide) => spans.push(glyph(guide)),
                None => {
                    let style = if brackets.contains(&column) {
                        span.style.patch(mark)
                    } else {
                        span.style
                    };
                    // Every space as a dot, wherever it is — between two words
                    // and inside a string as much as in the indentation. What
                    // it is for is seeing the shape of a line, and a space the
                    // eye has to infer is the one that hides a stray one.
                    //
                    // One glyph for one space, so every column is where it
                    // was: the passes below count columns, and the caret and
                    // the hit-test are counted against the same width.
                    //
                    // Not tabs. A tab is one character and any number of
                    // columns, so a glyph in its cell would put the rest of
                    // the line somewhere it is not — the same reason a
                    // tab-indented file is given no guides.
                    match character {
                        ' ' => spans.push(Span::styled("\u{00b7}", style.patch(faint))),
                        _ => spans.push(Span::styled(character.to_string(), style)),
                    }
                }
            }
            column += 1;
        }
    }
    // A blank line inside a block has no character to substitute into, so its
    // guides are laid down past the end of it rather than dropped — a guide
    // that breaks over an empty line is the one place the eye needs it most.
    for guide in guides {
        if guide.column < column {
            continue;
        }
        spans.push(Span::raw(" ".repeat(guide.column - column)));
        spans.push(glyph(guide));
        column = guide.column + 1;
    }
    Line::from(spans).style(line.style)
}

/// The line with the characters between two columns marked — reversed for the
/// selection, coloured for a search match. Cut per character: an edge falls
/// wherever it falls, not on a token boundary.
fn picked(
    line: &Line<'static>,
    from: usize,
    to: usize,
    mark: Style,
    has_gutter: bool,
) -> Line<'static> {
    let mut spans = Vec::new();
    let mut column = 0;
    for (index, span) in line.spans.iter().enumerate() {
        // The line number is not text you can select.
        if index == 0 && has_gutter {
            spans.push(span.clone());
            continue;
        }
        for character in span.content.chars() {
            let style = if (from..to).contains(&column) {
                span.style.patch(mark)
            } else {
                span.style
            };
            spans.push(Span::styled(character.to_string(), style));
            column += 1;
        }
    }
    Line::from(spans)
}

/// One line's gutter: the number, the column a bar may take, and the pad.
///
/// The line the cursor is on is drawn brighter, the way every editor draws it
/// — a column of identical greys says where you are nowhere, and the caret is
/// one cell in a screen full of text.
fn numbered(number: usize, cursor: Option<usize>) -> Line<'static> {
    let colour = match cursor == Some(number) {
        true => Color::White,
        false => Color::DarkGray,
    };
    Line::from(Span::styled(
        format!(" {number:>4} {PAD}"),
        Style::default().fg(colour),
    ))
}

/// Colour is a theme's business, which is why no scenario asserts it.
fn colour(kind: Kind, dark: bool) -> Color {
    match (kind, dark) {
        (Kind::Keyword, _) => Color::Magenta,
        (Kind::Operator, _) => Color::LightRed,
        (Kind::String, _) => Color::Green,
        (Kind::Comment, true) => Color::DarkGray,
        (Kind::Comment, false) => Color::Gray,
        (Kind::Number, _) => Color::Yellow,
        (Kind::Constant, _) => Color::LightYellow,
        (Kind::Function, _) => Color::Blue,
        (Kind::Type, _) => Color::Cyan,
        (Kind::Property, _) => Color::LightCyan,
        (Kind::Attribute, _) => Color::Indexed(214),
        (Kind::Markup, _) => Color::LightGreen,
        (Kind::Invalid, _) => Color::Red,
        (Kind::Punctuation, true) => Color::Indexed(245),
        (Kind::Punctuation, false) => Color::Indexed(240),
        // Identifiers, not "text nobody claimed": no grammar in the extended set
        // scopes a local, a parameter or an object key as `variable`, so every
        // name in a file lands here. White is what made a whole buffer read as
        // brighter than its keywords; this is Dark+'s variable colour, which is
        // what the name in an editor beside Varde is drawn in.
        (Kind::Plain, true) => Color::Rgb(0x9c, 0xdc, 0xfe),
        (Kind::Plain, false) => Color::Rgb(0x00, 0x10, 0x80),
    }
}

/// Renders the terminal model cell by cell so colours survive.
/// `focused` rather than read off `state.focus`: the strip's splits are one
/// `Pane`, and only the one with the keyboard lights up or shows a pick.
fn terminal_widget(
    pane: &PtyPane,
    title: &str,
    focused: bool,
    state: &State,
    which: Pane,
) -> Paragraph<'static> {
    let screen = pane.screen();
    let (rows, columns) = screen.size();
    // Only this pane's, so a drag in the AI pane does not light up the terminal.
    let picked = state
        .selection
        .as_ref()
        .filter(|_| focused)
        .and_then(|selection| selection.screen_span(which));
    let lines: Vec<Line> = (0..rows)
        .map(|row| {
            let spans: Vec<Span> = (0..columns)
                .map(|column| {
                    let cell = screen.cell(row, column);
                    let text = cell.map(|c| c.contents()).unwrap_or_default();
                    let text = if text.is_empty() {
                        " ".to_string()
                    } else {
                        text.to_string()
                    };
                    let mut style = Style::default();
                    if let Some(cell) = cell {
                        style = style
                            .fg(convert(cell.fgcolor()))
                            .bg(convert(cell.bgcolor()));
                        if cell.bold() {
                            style = style.add_modifier(Modifier::BOLD);
                        }
                        if cell.inverse() {
                            style = style.add_modifier(Modifier::REVERSED);
                        }
                    }
                    // Without this a drag over a pty pane picks text and shows
                    // nothing, which reads as selection not working at all. A
                    // charwise span runs from its first column to its last in
                    // reading order, which is what comparing the pair does.
                    if let Some((from, to)) = picked {
                        let at = (row as usize + 1, column as usize + 1);
                        if at >= (from.line, from.column) && at <= (to.line, to.column) {
                            style = style.add_modifier(Modifier::REVERSED);
                        }
                    }
                    Span::styled(text, style)
                })
                .collect();
            Line::from(spans)
        })
        .collect();
    let border = match focused {
        true => Color::Cyan,
        false => Color::DarkGray,
    };
    Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title.to_string())
            .border_style(Style::default().fg(border)),
    )
}

/// The AI pane's empty state: a box you type a CLI into. No session means the
/// pane's job is to help you start one.
fn start_ai_widget(state: &State, draft: &str) -> Paragraph<'static> {
    let mut lines = vec![Line::from(""); 2];
    lines.push(Line::from(Span::styled(
        "start an AI CLI",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        format!("┌{}┐", "─".repeat(22)),
        Style::default().fg(Color::Cyan),
    )));
    lines.push(Line::from(Span::styled(
        format!("│ {:<20} │", format!("{draft}█")),
        Style::default().fg(Color::Cyan),
    )));
    lines.push(Line::from(Span::styled(
        format!("└{}┘", "─".repeat(22)),
        Style::default().fg(Color::Cyan),
    )));
    lines.push(Line::from(Span::styled(
        "Enter to start",
        Style::default().fg(Color::DarkGray),
    )));
    Paragraph::new(lines)
        .centered()
        .block(pane_block("ai", state, Pane::Ai))
}

/// Which colour a tree glyph gets. The kind is the library's decision and the
/// colour is this file's, same split as `token` below. The values are the
/// Material Icon Theme's own, which is the theme the icons are drawn to match —
/// except where Material gives two languages one colour (TypeScript, Python, C,
/// C++, C# and Docker are all one blue in it). A 32-pixel icon can afford that
/// because its shape carries the language; one cell of a tree pane cannot, so a
/// clash is broken here rather than inherited, and `no_two_icon_kinds_share_a_colour`
/// holds it that way.
///
/// One match, not two. This was `icon_colour` plus a `prose_icon_colour` holding
/// the other half, which meant every kind added had to be named twice — once for
/// its colour and once in an `unreachable!` arm — and put a runtime panic where
/// the compiler's exhaustiveness check belongs.
fn icon_colour(kind: varde::tree::IconKind) -> Color {
    use varde::tree::IconKind as Kind;
    match kind {
        Kind::Directory => Color::Rgb(0x90, 0xa4, 0xae),
        Kind::Source => Color::Rgb(0x4c, 0xaf, 0x50),
        Kind::Build => Color::Rgb(0xe5, 0x73, 0x73),
        Kind::Rust => Color::Rgb(0xff, 0x70, 0x43),
        Kind::JavaScript => Color::Rgb(0xff, 0xca, 0x28),
        Kind::TypeScript => Color::Rgb(0x02, 0x88, 0xd1),
        Kind::Python => Color::Rgb(0x29, 0xb6, 0xf6),
        Kind::Go => Color::Rgb(0x00, 0xac, 0xc1),
        Kind::Java => Color::Rgb(0xf4, 0x43, 0x36),
        Kind::Vue => Color::Rgb(0x41, 0xb8, 0x83),
        Kind::Svelte => Color::Rgb(0xff, 0x57, 0x22),
        Kind::Ruby => Color::Rgb(0xd3, 0x2f, 0x2f),
        Kind::Php => Color::Rgb(0x77, 0x7b, 0xb4),
        Kind::C => Color::Rgb(0x5c, 0x6b, 0xc0),
        Kind::Cpp => Color::Rgb(0x79, 0x86, 0xcb),
        Kind::CSharp => Color::Rgb(0x9c, 0x4d, 0xcc),
        Kind::Swift => Color::Rgb(0xff, 0x6e, 0x40),
        Kind::Sql => Color::Rgb(0xff, 0xb3, 0x00),
        Kind::Markup => Color::Rgb(0xe6, 0x51, 0x00),
        Kind::Style => Color::Rgb(0x7e, 0x57, 0xc2),
        Kind::Data => Color::Rgb(0xf9, 0xa8, 0x25),
        Kind::Doc => Color::Rgb(0x42, 0xa5, 0xf5),
        Kind::Pdf => Color::Rgb(0xef, 0x53, 0x50),
        Kind::Image => Color::Rgb(0x26, 0xa6, 0x9a),
        Kind::Media => Color::Rgb(0xff, 0x98, 0x00),
        Kind::Archive => Color::Rgb(0xaf, 0xb4, 0x2b),
        Kind::Shell => Color::Rgb(0x66, 0xbb, 0x6a),
        Kind::Spec => Color::Rgb(0x00, 0xbf, 0xa5),
        Kind::Lock => Color::Rgb(0xff, 0xd5, 0x4f),
        Kind::Git => Color::Rgb(0xe6, 0x4a, 0x19),
        Kind::Docker => Color::Rgb(0x1e, 0x88, 0xe5),
        Kind::Package => Color::Rgb(0xe5, 0x39, 0x35),
        Kind::Plain => Color::Rgb(0xb0, 0xbe, 0xc5),
    }
}

fn action_icon(action: &str) -> &'static str {
    match action {
        "new-file" => "\u{f067}",
        "new-directory" => "\u{f07b}",
        "go-here" => "\u{f0a9}",
        "copy-path" => "\u{f0c5}",
        "search-here" => "\u{f002}",
        varde::history::GO_TO => "\u{f0a9}",
        // One cell in every font, as ADR 0022 asks of a Chip's glyph: the
        // debugger's first row action, built after the rule.
        varde::debug::REMOVE => "\u{2715}",
        varde::debug::EDIT => "\u{270e}",
        varde::risk::REFACTOR => "\u{f0ad}",
        varde::risk::RECOMPUTE => "\u{f021}",
        varde::risk::START_LOOP => "\u{f04b}",
        varde::risk::STOP_LOOP => "\u{f04d}",
        _ => "\u{f1f8}",
    }
}

fn convert(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(index) => Color::Indexed(index),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Two steps, so say which one you are on: pick a type, then write the body.
fn comment_lines(state: &State, chrome: &Chrome) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some((file, from, to)) = state.comment_target() {
        let range = if from == to {
            format!("{from}")
        } else {
            format!("{from}-{to}")
        };
        lines.push(Line::from(Span::styled(
            format!("  {file}:{range}"),
            Style::default().fg(Color::DarkGray),
        )));
    }
    if chrome.comment_kind.is_empty() {
        lines.push(Line::from(Span::styled(
            "  pick a type:",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from("  (i)ssue  (n)ote"));
        lines.push(Line::from("  (s)uggestion  (c)omment"));
        lines.push(Line::from(Span::styled(
            "  press a letter · Esc to cancel",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!("  {}", chrome.comment_kind),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        // Every row of the body, not one line with a block glued to the end:
        // the body is a buffer and Enter is a newline in it, so a box that can
        // only draw one row is a box that hides what was typed. The caret sits
        // where the buffer's cursor is rather than after the last character,
        // which is the difference a motion inside the body has to be visible
        // as.
        if let Some(body) = state.comment.as_ref() {
            let caret_row = body.line.saturating_sub(1);
            for (number, line) in body.shown().split('\n').enumerate() {
                let mut spans = vec![Span::raw("  ")];
                match number == caret_row {
                    true => spans.extend(with_caret(line, body.column)),
                    false => spans.push(Span::raw(line.to_string())),
                }
                lines.push(Line::from(spans));
            }
        }
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                keys::COMMENT_BOX_KEYS
                    .iter()
                    .map(|(key, word)| format!("{key} {word}"))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ),
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines
}

/// A line drawn with a caret on the character at a 1-based column: the
/// character stays and is *reversed*, rather than being replaced by a block.
///
/// Every other caret in Varde sits past the end of what was typed — the name
/// box's, the search box's — so a block glued on hides nothing. This is the
/// first that can sit mid-text, and a block drawn over a character is a
/// character the writer can no longer read: moving back four words to fix a
/// typo would hide the letter being fixed.
///
/// By characters and not bytes: a caret placed at a byte offset lands mid-glyph
/// on the first non-ASCII comment somebody writes, and this is arithmetic at the
/// edge, so the test below is what keeps it from being arithmetic nobody
/// checked.
fn with_caret(line: &str, column: usize) -> Vec<Span<'static>> {
    let chars: Vec<char> = line.chars().collect();
    let at = column.saturating_sub(1).min(chars.len());
    let over = chars.get(at).copied().unwrap_or(' ');
    vec![
        Span::raw(chars[..at].iter().collect::<String>()),
        Span::styled(
            over.to_string(),
            Style::default().add_modifier(Modifier::REVERSED),
        ),
        Span::raw(
            chars[at.saturating_add(1).min(chars.len())..]
                .iter()
                .collect::<String>(),
        ),
    ]
}

/// Hits grouped by file, read top to bottom, with the selected one highlighted.
fn search_screen(frame: &mut Frame, state: &State) {
    let Some(search) = state.search.as_ref() else {
        return;
    };
    let area = rect(layout::search_box(frame.area().width, frame.area().height));
    frame.render_widget(Clear, area);

    let results = &search.results;
    let dark = state.editor_theme != "light";
    let hits = results.hits.len();
    let files = varde::search::files(results).len();
    // A scoped search that finds nothing has to say why it looked nowhere else,
    // or "0 hits" reads as "not in this project".
    let scope = match search.scope.as_deref() {
        Some(folder) => format!(" under {}/", folder.display()),
        None => String::new(),
    };
    let count = if results.truncated {
        format!("first {hits} hits — narrow the query{scope}")
    } else {
        format!(
            "{hits} hit{} in {files} file{}{scope}",
            if hits == 1 { "" } else { "s" },
            if files == 1 { "" } else { "s" }
        )
    };
    let query = search.query.shown();
    let completion = varde::search::completion(query, results)
        .map(|word| word[query.len().min(word.len())..].to_string())
        .unwrap_or_default();
    let (before, after) = at_caret(&search.query);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(" ? ", Style::default().fg(Color::Cyan)),
            Span::raw(before),
            Span::styled("█", Style::default().fg(Color::Cyan)),
            Span::raw(after),
            Span::styled(completion, Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(Span::styled(
            format!("   {count}"),
            Style::default().fg(Color::DarkGray),
        )),
        // The box's own keys, in the box rather than in its bottom border: a
        // right-aligned caption under a scrolling list is not where anybody
        // looks for the key that gets them through it. It takes the row that
        // separated the query from the list, so the box loses no result row to
        // chrome.
        Line::from(Span::styled(
            format!(
                "   {}",
                keys::SEARCH_KEYS
                    .iter()
                    .map(|(key, word)| format!("{key} {word}"))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ),
            Style::default().fg(Color::DarkGray),
        )),
    ];
    // The rows `layout` takes out of the box before the list starts. Asserted
    // rather than commented: no scenario covers the renderer, and a fourth row
    // here would leave the clamp measuring a list a row longer than the one
    // drawn.
    debug_assert_eq!(lines.len(), layout::SEARCH_HEADER as usize);

    // The rows the scroll was clamped against, so the box draws exactly what
    // the clamp measured.
    let list: Vec<Line> = varde::search::rows(results)
        .into_iter()
        .map(|row| match row {
            varde::search::Row::File(file) => Line::from(Span::styled(
                format!(" {file}"),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            varde::search::Row::Hit(index) => {
                let hit = &results.hits[index];
                let mut spans = vec![Span::styled(
                    format!("{:>6} ", hit.line),
                    Style::default().fg(Color::DarkGray),
                )];
                spans.extend(coloured(
                    &hit.file,
                    &hit.text,
                    dark,
                    index == search.selected,
                ));
                Line::from(spans)
            }
        })
        .collect();
    // Skipped rather than scrolled: the query and its count stay put, and the
    // widget's own offset would take them off the top of the box first.
    lines.extend(list.into_iter().skip(search.scroll));
    // After the list and not part of it: a row the clamp never counted is a row
    // that scrolls out of step with the rest.
    if results.hits.is_empty() && !query.is_empty() {
        lines.push(Line::from(Span::styled(
            " no matches",
            Style::default().fg(Color::DarkGray),
        )));
    }

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("SEARCH")),
        area,
    );
    frame.set_cursor_position((area.x + 3 + search.query.column as u16, area.y + 1));
}

/// A single line, coloured like the editor colours it. Highlighted out of
/// context, so a line inside a block comment reads as code — acceptable in a
/// result list, where you are scanning rather than reading.
fn coloured(file: &str, text: &str, dark: bool, selected: bool) -> Vec<Span<'static>> {
    let mark = match selected {
        true => Style::default().add_modifier(Modifier::REVERSED),
        false => Style::default(),
    };
    highlight::highlight(file, text)
        .iter()
        .flat_map(|line| spans(line, dark))
        .map(|span| Span::styled(span.content, span.style.patch(mark)))
        .collect()
}

/// The palette's second face, Tools. The words the scenarios assert on are the
/// core's — [`tools::Availability::as_str`] — and they are what is drawn, so a
/// row reads on screen as the scenario spells it. A row with nothing behind it
/// is the one worth the reader's eye, whether that is a command to install or a
/// gap to fill in; an available row is one the reader has not taken, so it is
/// quiet like an installed one.
fn tool_lines(state: &State, selected: usize, height: u16) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut at = 0;
    let mut group = None;
    for (index, row) in tools::rows(state).into_iter().enumerate() {
        if group != Some(row.kind) {
            group = Some(row.kind);
            lines.push(Line::from(Span::styled(
                match row.kind {
                    tools::Kind::Server => "Language servers",
                    tools::Kind::Formatter => "Formatters",
                    tools::Kind::Adapter => "Debug adapters",
                    tools::Kind::Requirement => "Requirements",
                    tools::Kind::Speech => "Speech",
                },
                Style::default().add_modifier(Modifier::BOLD),
            )));
        }
        let colour = match row.availability {
            tools::Availability::Installed | tools::Availability::Available => Color::DarkGray,
            tools::Availability::Missing
            | tools::Availability::Unmet { .. }
            | tools::Availability::Stopped
            | tools::Availability::Partial { .. }
            | tools::Availability::Unpackaged
            | tools::Availability::InstallFailed
            | tools::Availability::NeedsInstaller { .. } => WARNING,
        };
        // The state word says there is a gap; only configuration's own words
        // say what is in it, so the row carries them. A row that differs from
        // the template is said too: a corrected template never reaches it, so
        // this is where the reader learns there is a difference to read.
        let says = match &row.availability {
            tools::Availability::Partial { without } => format!("  no {without}"),
            tools::Availability::NeedsInstaller { installer } => format!("  needs {installer}"),
            tools::Availability::Unmet { needs } => format!("  needs {needs}"),
            _ => String::new(),
        };
        let differs = match row.origin {
            tools::Origin::Differs => "  differs from template",
            tools::Origin::Template | tools::Origin::Own => "",
        };
        let gap = format!("{says}{differs}");
        // The row the install key acts on, marked where every list in Varde
        // marks it. Nothing else distinguishes it: a box this narrow spends its
        // columns on the command.
        let cursor = match index == selected {
            true => '>',
            false => ' ',
        };
        if index == selected {
            at = lines.len();
        }
        lines.push(Line::from(vec![
            Span::raw(format!("{cursor} {:<22} {:<30} ", row.name, row.command)),
            Span::styled(row.availability.as_str(), Style::default().fg(colour)),
            Span::styled(gap, Style::default().fg(Color::DarkGray)),
        ]));
    }
    // Every template row is listed, so the list outgrows a terminal. Borders,
    // the blank and the footer take four rows.
    let mut lines = window_on(lines, at, height, 4);
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        keys::TOOL_LIST_KEYS
            .iter()
            .map(|(key, word)| format!("   {key}  {word}"))
            .collect::<String>(),
        Style::default().fg(Color::DarkGray),
    )));
    lines
}

/// The launch list: one row per Launch configuration, the branch picker's
/// shape, and a list with none says where one is written.
fn launch_lines(state: &State, selected: usize, height: u16) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = varde::debug::launches(state)
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let cursor = match index == selected {
                true => '>',
                false => ' ',
            };
            Line::from(format!("{cursor} {name}"))
        })
        .collect();
    if lines.is_empty() {
        lines.push(Line::from(
            "  No Launch configurations: name one as [launch.<name>] in a config file.",
        ));
    }
    // Borders, the blank and the footer.
    let mut lines = window_on(lines, selected, height, 4);
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        keys::LAUNCH_LIST_KEYS
            .iter()
            .map(|(key, word)| format!("   {key}  {word}"))
            .collect::<String>(),
        Style::default().fg(Color::DarkGray),
    )));
    lines
}

/// The branch picker: one row per branch, newest first, with the row Enter acts
/// on marked where every list in Varde marks it, over the footer naming the two
/// keys the list answers.
fn branch_lines(
    names: &[String],
    filter: &str,
    selected: usize,
    height: u16,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let cursor = match index == selected {
                true => '>',
                false => ' ',
            };
            Line::from(format!("{cursor} {name}"))
        })
        .collect();
    if lines.is_empty() {
        lines.push(Line::from(match filter.is_empty() {
            true => "  This repository has no branches.".to_string(),
            // An empty box over a filter reads as a repository with nothing in
            // it, which is a different thing to have found.
            false => format!("  No branch matches {filter:?}."),
        }));
    }
    // Borders, the blank, the filter and the footer.
    let mut lines = window_on(lines, selected, height, 5);
    lines.push(Line::from(""));
    // What was typed, always drawn: a filter narrows silently otherwise, and
    // the blank line is where a reviewer learns the list is typed into at all.
    lines.push(Line::from(Span::styled(
        match filter.is_empty() {
            true => format!("   {}", keys::BRANCH_FILTER_HINT),
            false => format!("   filter: {filter}"),
        },
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        keys::BRANCH_LIST_KEYS
            .iter()
            .map(|(key, word)| format!("   {key}  {word}"))
            .collect::<String>(),
        Style::default().fg(Color::DarkGray),
    )));
    lines
}

/// A row a key acts on that nobody can see is a keypress on something unread,
/// so a list taller than the screen keeps the part `at` is in. `chrome` is the
/// rows the box spends on its borders and its footer.
fn window_on(lines: Vec<Line<'static>>, at: usize, height: u16, chrome: u16) -> Vec<Line<'static>> {
    let room = height.saturating_sub(chrome).max(1) as usize;
    let start = (at + 1).saturating_sub(room);
    lines.into_iter().skip(start).take(room).collect()
}

/// A list whose rows offer a key or carry none — a heading, a gap, the cancel
/// line — the ones carrying none dimmed so they do not read as keys.
fn rows_lines(rows: Vec<(Option<char>, String)>) -> Vec<Line<'static>> {
    rows.into_iter()
        .map(|(key, row)| match key {
            Some(_) => Line::from(row),
            None => Line::from(Span::styled(row, Style::default().fg(Color::DarkGray))),
        })
        .collect()
}

/// The measure an overlay's prose is wrapped to: what the widest box the
/// screen allows can actually show, since `layout::overlay` caps the box at
/// the screen's own width. Deliberately not a reading measure — a line that
/// already fits is left exactly as it was, so this changes nothing except the
/// sentences that used to run off the side.
fn overlay_measure(width: u16) -> usize {
    width.saturating_sub(4).max(20) as usize
}

/// Breaks the lines that do not fit. Wrapped here rather than by
/// `Paragraph::wrap`, because the box is sized from the line count: a
/// paragraph that wraps at render time makes that count a lie, and the box
/// stays one row tall while the text needs three. `textwrap` breaks an
/// over-long word too, which a Site's stored code line can easily be.
///
/// Every overlay builds its prose as one span per line — the styled lines are
/// the short headings, which never reach here — so a wrapped line takes the
/// first span's style. Losing a style on a line nobody builds is better than
/// losing the words on the lines everybody does.
fn wrapped(lines: Vec<Line<'static>>, measure: usize) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    for line in lines {
        if line.width() <= measure {
            out.push(line);
            continue;
        }
        let style = line
            .spans
            .first()
            .map(|span| span.style)
            .unwrap_or_default();
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        // The leading indent every overlay line carries is kept on the
        // continuations, so a wrapped sentence still reads as one block.
        let indent: String = text.chars().take_while(|c| *c == ' ').collect();
        let options = textwrap::Options::new(measure)
            .initial_indent(&indent)
            .subsequent_indent(&indent);
        for piece in textwrap::wrap(text.trim_start(), options) {
            out.push(Line::from(Span::styled(piece.into_owned(), style)));
        }
    }
    out
}

fn overlay(frame: &mut Frame, title: &str, lines: Vec<Line<'static>>) {
    let area = frame.area();
    let lines = wrapped(lines, overlay_measure(area.width));
    let widest = lines
        .iter()
        .map(|line| line.width() as u16)
        .max()
        .unwrap_or(0);
    let box_area = rect(layout::overlay(
        area.width,
        area.height,
        lines.len() as u16,
        widest,
    ));
    frame.render_widget(Clear, box_area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title.to_string()),
        ),
        box_area,
    );
}

/// The one part of this module that is arithmetic rather than drawing, so the
/// one part with a test. Both cases here were real mistakes in the writing:
/// dropping characters from span 0 slid the line numbers off the left edge, and
/// rebuilding the line without its own style made a visual selection disappear
/// the moment the view scrolled sideways.
#[cfg(test)]
mod tests {
    use super::{
        action_icon, authorship_clause, branch_lines, buffer_title, cheatsheet_rows, code_lines,
        colour, diff_rows, editor_block, faint, guided, highlight, icon_colour, launch_lines,
        layout, paint_drag, pane_actions_title, preview_line, right_title, risk_lines, risk_title,
        shift, source_lines, status_line, story_title, title_room, tree_lines, truncate,
        with_breakpoint, with_caret, Block, Borders, Color, Kind, Line, Modifier, Place, Selection,
        Span, State, Style, Tone, UnicodeWidthStr, DIRTY, DOTS, WARNING,
    };
    use varde::risk::{Figure, Figures, Function, Metrics};

    /// F40's exact wording, which no scenario asserts: the hand that wrote the
    /// line and the day it did, two columns apart and one clear of the border
    /// each side. Pinned here because the scenarios assert the authorship and
    /// never the copy — and pinned narrow as well, because the name is the half
    /// that gives: a date cut short names the wrong day.
    #[test]
    fn the_border_names_the_author_and_keeps_the_date_whole_when_it_is_cut() {
        let mut state = State::default();
        state.git_installed = true;
        state.repo = Some(Vec::new());
        let path = std::path::PathBuf::from("/w/main.rs");
        state.current_buffer = Some(path.clone());
        state.buffers.insert(
            path.clone(),
            varde::editor::Buffer::open("fn main() {}\n", false, 4),
        );
        let traced =
            |committed: &str| varde::authorship::traced(Some(committed), "fn main() {}\n").into();
        state.traced = Some((path.clone(), 1, traced("fn main() {}\n")));
        state.authorship.insert(
            path.clone(),
            vec![varde::authorship::Authored {
                author: "Ada Lovelace".to_string(),
                date: "2026-01-05".to_string(),
            }]
            .into(),
        );

        assert_eq!(authorship_clause(&state, 40), " Ada Lovelace  2026-01-05 ");
        assert_eq!(authorship_clause(&state, 20), " Ada L\u{2026}  2026-01-05 ");
        // A border with no room for the date says nothing rather than half of one.
        assert_eq!(authorship_clause(&state, 12), "");

        // And the one clause with no date to keep, so nothing is cut against it.
        state.traced = Some((path, 1, traced("fn main() { run() }\n")));
        assert_eq!(authorship_clause(&state, 40), " Not committed yet ");
    }

    /// The two sentences an empty picker can say are different findings, and
    /// the only place either is spelled. A repository with no branches and a
    /// filter that matched none of them look identical on screen otherwise,
    /// which sends a reviewer looking for a repository problem they do not
    /// have. Pinned here because copy is what this draws and nothing else can
    /// fail on it.
    #[test]
    fn an_empty_picker_says_which_kind_of_empty_it_is() {
        let text = |lines: Vec<Line>| {
            lines
                .iter()
                .map(|line| {
                    line.spans
                        .iter()
                        .map(|span| span.content.as_ref())
                        .collect()
                })
                .collect::<Vec<String>>()
        };
        assert!(text(branch_lines(&[], "", 0, 26))
            .contains(&"  This repository has no branches.".to_string()));
        let filtered = text(branch_lines(&[], "zzz", 0, 26));
        assert!(filtered.contains(&"  No branch matches \"zzz\".".to_string()));
        // And what was typed is on screen either way, so a list that narrowed
        // silently is not mistaken for the whole of it.
        assert!(filtered.contains(&"   filter: zzz".to_string()));
        assert!(text(branch_lines(&["main".to_string()], "", 0, 26))
            .contains(&"   type to filter".to_string()));
    }

    /// A list longer than the screen keeps the selected row in the box, and
    /// the box no taller than the screen: the wheel walks it off the bottom
    /// otherwise, and the row Enter acts on is one nobody can see.
    #[test]
    fn a_long_branch_list_follows_its_selection() {
        let names: Vec<String> = (0..40).map(|at| format!("branch-{at}")).collect();
        let lines: Vec<String> = branch_lines(&names, "", 30, 20)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect();
        // Two borders, then the list, the blank, the filter hint and the keys.
        assert_eq!(lines.len() + 2, 20);
        assert_eq!(lines.first().map(String::as_str), Some("  branch-16"));
        assert_eq!(lines[14], "> branch-30");
    }

    /// The Launch list follows its selection for the same reason.
    #[test]
    fn a_long_launch_list_follows_its_selection() {
        let mut state = State::default();
        for at in 10..50 {
            state.launches.insert(
                format!("launch-{at}"),
                varde::startup::Launch {
                    adapter: "rust".to_string(),
                    request: "launch".to_string(),
                    args: serde_json::Map::new(),
                    reattach: false,
                },
            );
        }
        let lines: Vec<String> = launch_lines(&state, 30, 20)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect();
        // Two borders, then the list, the blank and the keys.
        assert_eq!(lines.len() + 2, 20);
        assert_eq!(lines.first().map(String::as_str), Some("  launch-25"));
        assert_eq!(lines[15], "> launch-40");
    }
    use varde::tree::{IconKind, Row};
    use varde::{DiffLine, Event};

    /// The one promise the border carries: after a pick it says where the
    /// reviewer is *and* where they were, and where they are comes off the poll
    /// — so a branch changed in the terminal pane changes the title rather than
    /// leaving it claiming the branch Varde chose.
    #[test]
    fn the_story_border_names_the_branch_the_poll_last_read() {
        assert_eq!(story_title(&State::default()), "story");
        let mut state = State::default();
        state.left_branch = Some("main".to_string());
        state.branch = Some("feature".to_string());
        assert_eq!(story_title(&state), "story  on feature (was main)");
        // The poll, not the pick: a `git switch` in the terminal pane is a
        // branch change Varde did not make and must not go on denying.
        state.branch = Some("something-else".to_string());
        assert_eq!(story_title(&state), "story  on something-else (was main)");
    }

    /// A file drawn with every mark the editor lays over code at once — folds
    /// open and shut, a linewise pick, a charwise pick and its echoes, search
    /// hits, diagnostics, change bars, Run marks and a slide to the right — as
    /// the state, its tokens and its Run marks. Twice, because a charwise pick
    /// and a linewise one are two different selections.
    fn decorated() -> Vec<(State, Vec<Vec<highlight::Token>>, Vec<usize>)> {
        let text: String = (1..=64)
            .map(|n| match n % 8 {
                0 => "\n".to_string(),
                1 => format!("fn step{n}(count: usize) {{\n"),
                2 | 3 => format!("    let count = count + {n}; // step\n"),
                4 => "    if count > 3 {\n".to_string(),
                5 => format!("        println!(\"{{count}} {n}\");\n"),
                6 => "    }\n".to_string(),
                _ => "}\n".to_string(),
            })
            .collect();
        let path = std::path::PathBuf::from("/w/main.rs");
        let mut state = State::default();
        state.current_buffer = Some(path.clone());
        let mut buffer = varde::editor::Buffer::open(&text, false, 4);
        buffer.folded = vec![9, 33];
        buffer.go_to_place(Place {
            line: 18,
            column: 9,
        });
        state.buffers.insert(path.clone(), buffer);
        let committed = text.replace("+ 11;", "+ 0;");
        state.traced = Some((
            path.clone(),
            1,
            varde::authorship::traced(Some(&committed), &text).into(),
        ));
        state.find = Some(varde::Find {
            query: varde::editor::Buffer::text_box("step"),
            origin: varde::Place { line: 1, column: 1 },
            case: varde::search::Case::Smart,
            keys: varde::FindKeys::Away,
        });
        state.diagnostics.insert(
            path.clone(),
            [(
                "rust".to_string(),
                [
                    (3, varde::lsp::Severity::Error),
                    (20, varde::lsp::Severity::Warning),
                ]
                .into_iter()
                .map(|(line, severity)| varde::lsp::Diagnostic {
                    line,
                    column: 9,
                    end_column: Some(13),
                    severity,
                    message: "no".to_string(),
                })
                .collect(),
            )]
            .into(),
        );
        let tokens = highlight::highlight("main.rs", &text);
        let marks = vec![1, 17, 41, 57];

        let mut linewise = state.clone();
        let buffer = linewise.buffers.get_mut(&path).expect("open");
        buffer.key('V');
        buffer.key('j');
        buffer.key('j');

        let mut charwise = state;
        charwise.selection = Some(Selection::Buffer {
            anchor: Place {
                line: 18,
                column: 9,
            },
            cursor: Place {
                line: 18,
                column: 13,
            },
        });
        charwise.editor_hscroll = 3;
        vec![
            (linewise, tokens.clone(), marks.clone()),
            (charwise, tokens, marks),
        ]
    }

    /// The editor pane `rows` tall, drawn the way `draw` draws it.
    fn editor_drawn(
        state: &State,
        tokens: &[Vec<highlight::Token>],
        marks: &[usize],
        rows: u16,
    ) -> ratatui::buffer::Buffer {
        use ratatui::widgets::Widget;
        let area = ratatui::layout::Rect::new(0, 0, 60, rows + 2);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        super::editor_widget(
            state,
            None,
            (tokens, marks),
            (&[], &[]),
            &[],
            area,
            faint(None),
        )
        .render(area, &mut buffer);
        buffer
    }

    /// #101: the editor draws only the rows the pane shows, and every row it
    /// draws has to be, cell for cell, the row the renderer that built the
    /// whole file and scrolled it drew there — the folds, the picks, the hits,
    /// the underlines and every mark in the gutter included. Scrolled past the
    /// top, through a fold and off the end, because the edges of a window are
    /// where a line found by its number lands on the wrong row.
    ///
    /// Against that renderer's own output, taken at the commit before #101 and
    /// committed beside the suite: a comparison with a whole-file render of
    /// today's code passes any regression the two paths share (#104). Its
    /// bottom border was redrawn once, when the in-file search's line came to
    /// stay on with its highlights (#79); every row of text inside it is still
    /// that renderer's.
    #[test]
    fn a_scrolled_pane_draws_what_the_whole_file_renderer_drew() {
        let before = include_str!("../tests/snapshots/editor_before_101.txt");
        let mut before = before.split_inclusive("\n}\n");
        for (index, (state, tokens, marks)) in decorated().into_iter().enumerate() {
            for scroll in [0, 1, 5, 7, 8, 17, 30, 44, 49, 60] {
                let mut scrolled = state.clone();
                scrolled.editor_scroll = scroll;
                let window = editor_drawn(&scrolled, &tokens, &marks, 14);
                let drawn = format!("fixture {index} scrolled {scroll}\n{window:?}\n");
                assert_eq!(Some(drawn.as_str()), before.next());
            }
        }
        assert_eq!(
            before.next(),
            None,
            "a render the snapshot has and this does not"
        );
    }

    /// #101: a frame of a long file costs what the pane shows rather than what
    /// the file holds — the editor builds and marks exactly the rows it has
    /// room for, the first of them the row it is scrolled to, and fewer only
    /// where the file runs out.
    #[test]
    fn the_editor_builds_only_the_rows_the_pane_shows() {
        let text = "let x = 1; // x\n".repeat(20_000);
        let path = std::path::PathBuf::from("/w/main.rs");
        let mut state = State::default();
        state.current_buffer = Some(path.clone());
        state.find = Some(varde::Find {
            query: varde::editor::Buffer::text_box("x"),
            origin: varde::Place { line: 1, column: 1 },
            case: varde::search::Case::Smart,
            keys: varde::FindKeys::Away,
        });
        state
            .buffers
            .insert(path.clone(), varde::editor::Buffer::open(&text, false, 4));
        let tokens = highlight::plain(&text);
        let area = ratatui::layout::Rect::new(0, 0, 60, 12);
        for (scroll, rows) in [(0, 10), (9_990, 10), (19_995, 6)] {
            state.editor_scroll = scroll;
            let lines = source_lines(
                &state,
                &path,
                &state.buffers[&path],
                (&tokens, &[]),
                area,
                faint(None),
            );
            assert_eq!(lines.len(), rows, "scrolled {scroll}");
            assert_eq!(
                lines[0].spans[0].content.trim(),
                (scroll + 1).to_string(),
                "scrolled {scroll}"
            );
        }
    }

    fn measured() -> State {
        let function = |file: &str, name: &str, line, cyclomatic| Function {
            file: file.to_string(),
            name: name.to_string(),
            line,
            metrics: Metrics {
                cyclomatic,
                ..Metrics::default()
            },
        };
        let mut state = State::default();
        state.risk_threshold = 20;
        state.risk.figure = Figure::Current(Figures {
            functions: vec![
                function("src/keys.rs", "route", 88, 31),
                function("src/ui.rs", "draw", 17, 22),
            ],
            unparsed: 0,
        });
        state
    }

    /// One row per Function and no chrome inside the borders: the pane's row
    /// count is its height less its two border rows, which is what keeps its
    /// last row reachable. Everything the pane says about the figure is on the
    /// border instead — the pane is narrower than a path, so rows inside it are
    /// for rows.
    #[test]
    fn the_risk_pane_spends_every_row_inside_its_borders_on_a_function() {
        let state = measured();
        let lines = risk_lines(&state, 30);
        assert_eq!(lines.len(), 2, "a row that is not a Function: {lines:?}");
        let drawn: Vec<String> = lines.iter().map(Line::to_string).collect();
        assert!(drawn[0].starts_with(" route"), "{drawn:?}");
        // The selected row's figure is followed by its one action icon; every
        // other row ends on the figure.
        assert!(drawn[0].contains("31"), "{drawn:?}");
        assert!(drawn[1].starts_with(" draw"), "{drawn:?}");
        assert!(drawn[1].ends_with("22"), "{drawn:?}");
        // The path is not in the row: the pane is narrower than one.
        assert!(
            drawn.iter().all(|row| !row.contains("src/")),
            "a path in a row: {drawn:?}"
        );
        // Every row is exactly the pane's inner width, so the figures line up
        // down the right-hand edge rather than trailing each name.
        assert!(
            drawn.iter().all(|row| row.chars().count() == 28),
            "{drawn:?}"
        );
    }

    /// A row says where its Function is, not only what it is called: acting on
    /// a row means going there, and the row already knew the line. It joins the
    /// figures hard against the right-hand border rather than trailing the name,
    /// so the column reads down the pane — and it leads within that group,
    /// because a row is truncated from the right and the line is the half that
    /// must survive a pane too narrow for everything.
    #[test]
    fn a_risk_row_names_the_line_its_function_starts_on() {
        let state = measured();
        let drawn: Vec<String> = risk_lines(&state, 30).iter().map(Line::to_string).collect();
        assert!(drawn[1].ends_with(":17 22"), "{drawn:?}");
        // Too narrow to draw the name: the line is still there, and the name is
        // what gave way.
        let narrow: Vec<String> = risk_lines(&state, 12).iter().map(Line::to_string).collect();
        assert!(narrow[1].contains(":17"), "{narrow:?}");
        assert!(!narrow[1].contains("draw"), "{narrow:?}");
        // With no columns left for a name at all, none is drawn rather than an
        // ellipsis: a row one column over the pane's inner width slides the
        // action icons off the columns the mouse hit-tests them from.
        assert_eq!(risk_lines(&state, 9).remove(1).to_string(), ":17 22");
        // Dimmed rather than drawn in the figure's colour, so the two are not
        // read as one number.
        let spans = risk_lines(&state, 30).remove(1).spans;
        assert_eq!(spans[1].content, ":17 ");
        assert_eq!(spans[1].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[2].style.fg, Some(WARNING));
    }

    /// A Function the analyser found nothing to count in is still somewhere,
    /// and the row still says where: the line comes off the Function itself, so
    /// a figure of zero suppresses nothing.
    #[test]
    fn a_row_with_nothing_measured_still_names_its_line() {
        let mut state = measured();
        state.risk_all = true;
        // Past the one row, so the row draws no action icons and the line is
        // what the row ends on.
        state.risk_selection = 1;
        state.risk.figure = Figure::Current(Figures {
            functions: vec![Function {
                file: "src/lib.rs".to_string(),
                name: "empty".to_string(),
                line: 5,
                metrics: Metrics::default(),
            }],
            unparsed: 0,
        });
        let drawn = risk_lines(&state, 30)[0].to_string();
        assert!(drawn.ends_with(":5 0"), "{drawn}");
    }

    /// The pane's action icons land on the columns `mouse::pane_action_at`
    /// hit-tests: two per icon, hard against the top-right corner, inside the
    /// border. Rendered rather than reasoned about, because the placement is
    /// ratatui's and not ours — the same reason `layout`'s tests pin the
    /// rectangles its solver produced.
    #[test]
    fn the_panes_action_icons_land_on_the_columns_they_are_hit_tested_from() {
        use ratatui::widgets::Widget;
        let state = measured();
        let area = ratatui::layout::Rect::new(0, 0, 30, 4);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        super::pane_block("risk", &state, varde::Pane::Risk)
            .title(pane_actions_title(&state))
            .render(area, &mut buffer);
        let top: Vec<String> = (0..30)
            .map(|column| buffer[(column, 0)].symbol().to_string())
            .collect();
        let icons: Vec<String> = varde::risk::pane_actions(&state)
            .into_iter()
            .flat_map(|action| [super::action_icon(action).to_string(), " ".to_string()])
            .collect();
        // Two icons, so the four columns before the corner: 25..=28.
        assert_eq!(top[25..29], icons[..], "{top:?}");
        // Quiet until the arrows step onto them, and lit once they have: the
        // keyboard reaches these by stepping past the last row, and an armed
        // action drawn the same grey as the one beside it is a position nothing
        // on screen shows. Not the row icons' `selected_action` alone — the same
        // field arms a row's own icon, so the styling has to read where the
        // keyboard is as well.
        assert_eq!(buffer[(25, 0)].style().fg, Some(Color::DarkGray));
        let mut armed = state;
        armed.focus = varde::Pane::Risk;
        // Past both rows: the pane's actions.
        armed.risk_selection = 2;
        armed.selected_action = Some(1);
        assert!(varde::risk::on_actions(&armed));
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        super::pane_block("risk", &armed, varde::Pane::Risk)
            .title(pane_actions_title(&armed))
            .render(area, &mut buffer);
        assert_eq!(buffer[(25, 0)].style().fg, Some(Color::DarkGray));
        assert_eq!(buffer[(27, 0)].style().fg, Some(Color::Cyan), "the loop");

        // The same field arms a *row's* own icon, so reading it alone would
        // light one of these whenever a row's refactor was armed — two panes'
        // worth of icons claiming the keyboard at once.
        let mut on_a_row = armed;
        on_a_row.risk_selection = 0;
        on_a_row.selected_action = Some(0);
        assert!(!varde::risk::on_actions(&on_a_row));
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        super::pane_block("risk", &on_a_row, varde::Pane::Risk)
            .title(pane_actions_title(&on_a_row))
            .render(area, &mut buffer);
        assert_eq!(buffer[(25, 0)].style().fg, Some(Color::DarkGray));
    }

    /// R35.8. The Transport lands on the columns `mouse::transport_at`
    /// hit-tests: each Chip followed by a column of border, the last against
    /// the top-right corner, in both of its shapes. Rendered rather than
    /// reasoned about, because the placement is ratatui's and not ours — the
    /// same reason `layout`'s tests pin the rectangles its solver produced.
    #[test]
    fn the_transports_chips_land_on_the_columns_they_are_hit_tested_from() {
        use ratatui::widgets::Widget;
        let mut state = State::default();
        state.current_buffer = Some(std::path::PathBuf::from("/w/guide.md"));
        state.speech.speed = 1.25;
        let chips = varde::reading::transport(&state);
        for (width, drawn) in [
            (40, " \u{25ba} \u{2500} \u{ab} \u{2500} \u{bb} \u{2500} \u{25a0} \u{2500} 1.25x \u{2500}"),
            (
                120,
                " \u{25ba} :pause \u{2500} \u{ab} :prev \u{2500} \u{bb} :next \u{2500} \u{25a0} :stop \u{2500} 1.25x :speed \u{2500}",
            ),
        ] {
            let area = ratatui::layout::Rect::new(0, 0, width, 4);
            let mut buffer = ratatui::buffer::Buffer::empty(area);
            Block::default()
                .borders(Borders::ALL)
                // No Authorship on a default State — nothing told the core git
                // is installed — so there is no clause to leave room for.
                .title(right_title(&state, 0, width))
                .render(area, &mut buffer);
            let top: String = (0..width)
                .map(|column| buffer[(column, 0)].symbol().to_string())
                .collect();
            assert!(top.ends_with(&format!("{drawn}\u{2510}")), "{top:?}");
            // And every column of it answers with the Chip drawn there, every
            // column of border between two with none.
            let labels = layout::chip_labels(&chips, width, layout::EDITOR_TITLE);
            let area = layout::Area {
                x: 0,
                y: 0,
                width,
                height: 4,
            };
            let start = width - 1 - drawn.chars().count() as u16;
            for (at, cell) in drawn.chars().enumerate() {
                let column = start + at as u16;
                let hit = layout::strip_at(area, &labels, column).map(|hit| chips[hit].action);
                let owner = drawn.chars().take(at + 1).filter(|c| *c == '\u{2500}').count();
                let expected = match cell {
                    '\u{2500}' => None,
                    _ => Some(chips[owner].action),
                };
                assert_eq!(hit, expected, "{width}: column {column}");
            }
        }
    }

    /// No scenario can see a colour, so this is where "the theme's named
    /// colours, never fixed RGB" is held: every tone of every hue, under the
    /// pointer and not.
    #[test]
    fn every_chip_is_drawn_in_the_themes_named_colours() {
        let mut state = State::default();
        state.current_buffer = Some(std::path::PathBuf::from("/w/guide.md"));
        let named = |colour: Option<Color>| {
            !matches!(colour, Some(Color::Rgb(..)) | Some(Color::Indexed(_)))
        };
        for lit in [
            None,
            Some(varde::reading::STOP),
            Some(varde::reading::SPEED),
        ] {
            for hovered in [None, Some(varde::reading::PREVIOUS)] {
                state.transport_lit = lit;
                state.hovered_action = hovered;
                for span in right_title(&state, 0, 120).spans {
                    let style = span.style;
                    assert!(named(style.fg) && named(style.bg), "{span:?}");
                }
            }
        }
        // And lit is not a colour of its own but the Chip reversed, which is
        // what keeps it in the theme whatever the theme is.
        state.transport_lit = Some(varde::reading::STOP);
        state.hovered_action = None;
        let spans = right_title(&state, 0, 120).spans;
        let stop = spans
            .iter()
            .find(|span| span.content.contains('\u{25a0}'))
            .expect("the stop Chip");
        assert_eq!(stop.style.fg, Some(Color::Red));
        assert!(stop.style.add_modifier.contains(Modifier::REVERSED));

        // Plain, each glyph in its hue and its keys dimmer beside it; under the
        // pointer, bold and underlined as well, since nothing else says it is a
        // button. Play is plain while a Reading is paused, previous with one.
        state.transport_lit = None;
        state.hovered_action = Some(varde::reading::PREVIOUS);
        state.reading = Some(varde::reading::Reading {
            utterances: varde::reading::utterances("One."),
            offsets: Vec::new(),
            at_ms: 0,
            paused: true,
            file: None,
        });
        let spans = right_title(&state, 0, 120).spans;
        let drawn = |glyph: char| {
            let at = spans
                .iter()
                .position(|span| span.content.contains(glyph))
                .expect("the Chip");
            (spans[at].style, spans[at + 1].style)
        };
        for (glyph, hue) in [
            ('\u{25ba}', Color::Green),
            ('\u{ab}', Color::Blue),
            ('\u{bb}', Color::Blue),
        ] {
            let (head, keys) = drawn(glyph);
            assert_eq!((head.fg, keys.fg), (Some(hue), Some(Color::DarkGray)));
        }
        let hovered = Modifier::BOLD | Modifier::UNDERLINED;
        assert!(drawn('\u{ab}').0.add_modifier.contains(hovered));
        assert!(!drawn('\u{bb}').0.add_modifier.intersects(hovered));
        state.reading.as_mut().expect("a Reading").paused = false;
        let spans = right_title(&state, 0, 120).spans;
        let pause = spans
            .iter()
            .find(|span| span.content.contains('\u{25ae}'))
            .expect("the pause Chip");
        assert_eq!(pause.style.fg, Some(Color::Yellow));
    }

    /// F40 on the border, rendered: the Authorship lands to the *left* of the
    /// Transport, and the filename is cut before either. Rendered rather than
    /// reasoned about, for the reason the Transport's own placement is — and the
    /// Transport still ends hard against the corner, which is where
    /// `layout::strip_at` hit-tests it from: a clause drawn into those columns
    /// would be a control that can be clicked and not seen.
    #[test]
    fn the_authorship_sits_between_the_filename_and_the_transport() {
        use ratatui::widgets::Widget;
        let mut state = State::default();
        state.git_installed = true;
        state.repo = Some(Vec::new());
        state.speech.speed = 1.0;
        let path = std::path::PathBuf::from("/w/guide.md");
        state.current_buffer = Some(path.clone());
        state.buffers.insert(
            path.clone(),
            varde::editor::Buffer::open("# Guide\n", false, 4),
        );
        state.traced = Some((
            path.clone(),
            1,
            varde::authorship::traced(Some("# Guide\n"), "# Guide\n").into(),
        ));
        state.authorship.insert(
            path,
            vec![varde::authorship::Authored {
                author: "Ada Lovelace".to_string(),
                date: "2026-01-05".to_string(),
            }]
            .into(),
        );

        // Through `editor_block`, so what is pinned is the border the editor
        // actually draws: sharing out the room is its arithmetic, and a test
        // that restated it would pass while the border went wrong.
        let area = ratatui::layout::Rect::new(0, 0, 80, 4);
        let drawn = |state: &State, name: &str| {
            let mut buffer = ratatui::buffer::Buffer::empty(area);
            let title = buffer_title(
                name,
                &varde::editor::Buffer::open("x", false, 4),
                "normal",
                title_room(state, 80),
            );
            editor_block(state, title, Vec::new(), None, 80).render(area, &mut buffer);
            (0..80)
                .map(|column| buffer[(column, 0)].symbol().to_string())
                .collect::<String>()
        };
        let top = drawn(&state, "guide.md");
        assert!(top.starts_with("\u{250c}guide.md"), "{top:?}");
        assert!(
            top.contains("Ada Lovelace  2026-01-05"),
            "the authorship is not on the border: {top:?}"
        );
        assert!(top.ends_with(" 1.00x \u{2500}\u{2510}"), "{top:?}");

        // And the filename is the last thing to give: a name that leaves no room
        // for a date takes the columns, rather than being drawn over by a clause
        // about who wrote a line in a file nobody can now name.
        let top = drawn(
            &state,
            "a-very-long-document-name-indeed-and-then-some-more-of-it.md",
        );
        assert!(top.contains("indeed-and-then"), "the name gave: {top:?}");
        assert!(!top.contains("Ada"), "{top:?}");
        assert!(top.ends_with(" 1.00x \u{2500}\u{2510}"), "{top:?}");
    }

    /// A filename long enough to reach the Transport is cut, rather than drawn
    /// over controls that can then be clicked and not seen: ratatui draws a
    /// left-aligned title over a right-aligned one.
    #[test]
    fn a_long_filename_is_truncated_rather_than_covering_a_control() {
        use ratatui::widgets::Widget;
        let mut state = State::default();
        state.current_buffer = Some(std::path::PathBuf::from("/w/guide.md"));
        state.speech.speed = 1.0;
        let area = ratatui::layout::Rect::new(0, 0, 40, 4);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        let name = "a-very-long-document-name-indeed.md";
        Block::default()
            .borders(Borders::ALL)
            .title(buffer_title(
                name,
                &varde::editor::Buffer::open("x", false, 4),
                "normal",
                title_room(&state, 40),
            ))
            // No Authorship on a default State — nothing told the core git is
            // installed — so there is no clause to leave room for.
            .title(right_title(&state, 0, 40))
            .render(area, &mut buffer);
        let top: String = (0..40)
            .map(|column| buffer[(column, 0)].symbol().to_string())
            .collect();
        assert!(top.contains('\u{2026}'), "the name was not cut: {top:?}");
        assert!(top.ends_with(" 1.00x \u{2500}\u{2510}"), "{top:?}");
        // And a column of border between the cut title and the first
        // control, so the two do not read as one word.
        assert!(top.contains("[normal]\u{2500} \u{25ba}"), "{top:?}");
    }

    /// A running loop's line and the icons share the border: the left title is
    /// cut to the columns the icons leave, so the stop is never a button that
    /// can be clicked and not seen — and the caption still fits, because a wait
    /// nobody can read is the hang the caption exists to rule out.
    #[test]
    fn a_running_loop_says_what_it_waits_for_without_covering_the_stop() {
        use ratatui::widgets::Widget;
        let mut state = measured();
        state.max_iterations = 3;
        state.refactor.running = Some(varde::risk::Iteration {
            scope: varde::risk::Scope::Workspace,
            number: 1,
            test_command: "cargo test".to_string(),
            wait: varde::risk::Wait::Session,
            before: Some(Figures::default()),
        });
        let area = ratatui::layout::Rect::new(0, 0, 30, 4);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        super::risk_widget(&state, 30).render(area, &mut buffer);
        let top: Vec<String> = (0..30)
            .map(|column| buffer[(column, 0)].symbol().to_string())
            .collect();
        assert_eq!(top[1..18].concat(), "risk  1/3 session", "{top:?}");
        assert_eq!(
            top[25..29].concat(),
            format!(
                "{} {} ",
                super::action_icon(varde::risk::RECOMPUTE),
                super::action_icon(varde::risk::STOP_LOOP)
            ),
            "{top:?}"
        );
    }

    /// The half of the ask no Scenario can see: a Buffers row's mark is the
    /// same glyph, in the same colour, that the editor's dot strip draws for
    /// that buffer — read off both renderers and compared, rather than each
    /// asked to agree with the same constant. Two views of one fact, so a
    /// buffer reading as unsaved on the bottom edge and merely open in the pane
    /// is the disagreement this closes.
    #[test]
    fn a_buffers_row_carries_the_dot_the_editor_draws() {
        let mut state = State::default();
        state.root = std::path::PathBuf::from("/w");
        for name in ["src/one.rs", "src/two.rs"] {
            state = varde::update(
                &state,
                Event::BufferOpened {
                    path: state.root.join(name),
                    contents: "contents".to_string(),
                    preview: false,
                    at: None,
                },
            )
            .0;
        }
        state
            .buffers
            .get_mut(&state.root.join("src/one.rs"))
            .expect("the buffer")
            .draft = Some("edited".to_string());
        let dots = super::dot_spans(&state);
        let rows = super::buffers_lines(&state, 30);
        assert_eq!(rows.len(), 2);
        for (index, path) in varde::buffer_list(&state).into_iter().enumerate() {
            let dot = &dots[index * 2];
            let mark = &rows[index].spans[0];
            assert_eq!((&mark.content, mark.style.fg), (&dot.content, dot.style.fg));
            // And the row says which file it is, which the dot cannot — by
            // name alone. The path is on the border, where a row's width
            // does not cut it short.
            let name = path.file_name().expect("a name").to_str().expect("utf-8");
            assert_eq!(
                rows[index].spans[1].content,
                format!(" {name}"),
                "{:?}",
                rows[index]
            );
        }
        // Not the same mark for both, or the comparison above would hold for a
        // renderer that drew one dot for everything. Colour, because unsaved
        // and current are the same filled circle and only the colour tells
        // them apart — which is the pair the strip carries too.
        assert_ne!(rows[0].spans[0].style.fg, rows[1].spans[0].style.fg);
    }

    /// `mouse::risk_action_at` hit-tests the row's action icon from the pane's
    /// right edge, so what comes before it has to fill exactly the columns the
    /// row reserved — getting that wrong lands the click one column off, which
    /// is the bug the tree's own version of this test guards.
    #[test]
    fn the_risk_row_fills_the_columns_its_action_is_hit_tested_from() {
        let state = measured();
        let line = risk_lines(&state, 30).remove(state.risk_selection);
        assert_eq!(
            varde::risk::row_actions(&state).len(),
            1,
            "one action, so one two-column cell"
        );
        let before: usize = line.spans[..line.spans.len() - 2]
            .iter()
            .map(|span| span.content.chars().count())
            .sum();
        // width - 2 for the borders, less two columns for the action.
        assert_eq!(before, (30 - 2) - 2);
    }

    /// The Paused line's marker takes the Breakpoint column and its wash runs
    /// to the pane's right border, however short the line.
    #[test]
    fn the_paused_line_is_marked_and_washed_across_the_pane() {
        let plain = Line::from(vec![Span::raw("   3 "), Span::raw("x")]);
        let line = super::on_paused_line(plain, varde::debug::Why::Paused, 30, true);
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(text.starts_with("\u{2192}  3 x"), "{text:?}");
        assert_eq!(text.width(), 30 - 2);
        assert!(line.spans.iter().all(|span| span.style.bg.is_some()));
    }

    /// A Diagnostic's row is its start and the first line of its message, cut
    /// with `…` to the pane, under a heading naming its file.
    #[test]
    fn a_diagnostic_row_is_cut_to_the_pane_under_its_file() {
        let mut state = State::default();
        state.root = std::path::PathBuf::from("/w");
        state.corner = varde::layout::Corner::Diagnostics(varde::lsp::Severity::Error);
        state.diagnostics.insert(
            std::path::PathBuf::from("/w/src/a.rs"),
            [(
                "rust".to_string(),
                vec![varde::lsp::Diagnostic {
                    line: 12,
                    column: 5,
                    end_column: None,
                    severity: varde::lsp::Severity::Error,
                    message: "mismatched types: expected u8\nfound u16".to_string(),
                }],
            )]
            .into_iter()
            .collect(),
        );
        let text: Vec<String> = super::diagnostics_lines(&state, 24)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect()
            })
            .collect();
        assert_eq!(text, [" src/a.rs", "   12:5 mismatched ty…"]);
    }

    /// The Breakpoint list's row: its path and line, `stale` when it is, and
    /// the icon in the two columns `mouse::breakpoint_action_at` hit-tests —
    /// however long the path is.
    #[test]
    fn the_breakpoint_row_names_its_line_and_keeps_its_icon_in_place() {
        let mut state = State::default();
        state.root = std::path::PathBuf::from("/w");
        state.breakpoints = vec![varde::debug::Breakpoint {
            file: std::path::PathBuf::from("/w/src/a/very/long/path/to/main.rs"),
            line: 3,
            text: String::new(),
            stale: true,
            properties: Default::default(),
        }];
        let line = super::breakpoints_lines(&state, 30).remove(0);
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(text.contains(":3 stale"), "{text:?}");
        let before: usize = line.spans[..line.spans.len() - 2]
            .iter()
            .map(|span| span.content.width())
            .sum();
        assert_eq!(before, (30 - 2) - 2, "{line:?}");
    }

    /// `mouse::history_action_at` hit-tests the row's icon from the pane's right
    /// edge, so what comes before it has to fill exactly the columns the row
    /// reserved — the same contract the Risk row's own test above pins, and the
    /// same off-by-one it exists to catch.
    #[test]
    fn the_history_row_fills_the_columns_its_action_is_hit_tested_from() {
        let mut state = State::default();
        state.root = std::path::PathBuf::from("/w");
        state.visits = vec![varde::history::Visit {
            file: "src/editor.rs".to_string(),
            line: 29,
            column: 1,
            text: "pub struct Buffer {".to_string(),
        }];
        state.history_selection = 0;
        let line = super::history_lines(&state, 30).remove(0);
        assert_eq!(
            varde::history::row_actions(&state).len(),
            1,
            "one action, so one two-column cell"
        );
        let before: usize = line.spans[..line.spans.len() - 2]
            .iter()
            .map(|span| span.content.chars().count())
            .sum();
        // width - 2 for the borders, less two columns for the action.
        assert_eq!(before, (30 - 2) - 2, "{line:?}");
        // And the row says the three things the ask names, in that order: the
        // file's name, the excerpt, then the line — the line last, so a pane
        // too narrow for both truncates the excerpt rather than where to go.
        // Wider than the tree's default thirty columns, which is not wide
        // enough for a whole excerpt and cuts it: what survives a narrow pane
        // is the half that says where, which is the rule the order encodes.
        let drawn: String = super::history_lines(&state, 44)
            .remove(0)
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(
            drawn.starts_with(" editor.rs: pub struct Buffer..."),
            "{drawn:?}"
        );
        assert!(drawn
            .trim_end()
            .ends_with(action_icon(varde::history::GO_TO)));
        assert!(drawn.contains(" 29"), "{drawn:?}");
        // Every width from the narrowest that holds the line and the icon
        // upwards fills the row exactly: a pane too narrow for the name spends
        // its columns on the line and the icon — the half that says where — and
        // never one column more than it has, which would slide the icon off the
        // columns it is hit-tested from. `truncate` answers an ellipsis even
        // for no room, so the row has to refuse to draw one at all.
        for width in 7..44u16 {
            let row = super::history_lines(&state, width).remove(0);
            let cells: usize = row
                .spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum();
            assert_eq!(
                cells,
                width as usize - 2,
                "{width} columns drew {cells}: {row:?}"
            );
        }
    }

    /// What the border says that a row cannot: the whole relative path of the
    /// row the keyboard is on. Pinned the way `risk_title` is, for the same
    /// reason — the row carries the file's *name*, so the border is the pane's
    /// only answer to which file a row is in, and a path drawn over the pane's
    /// own edge is a path nobody can read.
    #[test]
    fn the_history_border_names_the_file_of_the_row_the_marker_is_on() {
        let mut state = State::default();
        state.root = std::path::PathBuf::from("/w");
        let visit = |file: &str| varde::history::Visit {
            file: file.to_string(),
            line: 29,
            column: 1,
            text: "pub struct Buffer {".to_string(),
        };
        state.visits = vec![visit("src/editor.rs"), visit("src/keys/reserved.rs")];
        // The position past the newest Visit: nowhere travelled to, so no row
        // is standing on and the border says only the pane's name.
        state.history_selection = state.visits.len();
        assert_eq!(super::history_title(&state, 40), "history");
        state.history_selection = 0;
        assert_eq!(super::history_title(&state, 40), "history  src/editor.rs");
        // The marker moves and the border follows it: the two cannot disagree
        // about which row you are on, since the row itself cannot say.
        state.history_selection = 1;
        assert_eq!(
            super::history_title(&state, 40),
            "history  src/keys/reserved.rs"
        );
        // And at the width the pane actually opens at, cut to the columns
        // inside the borders rather than drawn across them.
        assert_eq!(
            super::history_title(&state, 30),
            "history  src/keys/reserved.…"
        );
    }

    /// A row whose line has moved on still says what it recorded and stops
    /// claiming to be current — the excerpt goes dim rather than being coloured
    /// as the code it no longer is. Only a file still open can be asked.
    #[test]
    fn a_stale_history_row_is_dimmed_rather_than_coloured() {
        let mut state = State::default();
        state.root = std::path::PathBuf::from("/w");
        state = varde::update(
            &state,
            Event::BufferOpened {
                path: state.root.join("src/editor.rs"),
                contents: "pub struct Buffer {\n".to_string(),
                preview: false,
                at: None,
            },
        )
        .0;
        let visit = |text: &str| varde::history::Visit {
            file: "src/editor.rs".to_string(),
            line: 1,
            column: 1,
            text: text.to_string(),
        };
        state.visits = vec![visit("pub struct Buffer {")];
        let current = super::history_lines(&state, 44).remove(0);
        state.visits = vec![visit("pub struct Nothing {")];
        let stale = super::history_lines(&state, 44).remove(0);
        assert_ne!(current.spans[1].style.fg, stale.spans[1].style.fg);
        assert_eq!(stale.spans[1].style.fg, Some(Color::DarkGray));
        // Still the text it recorded, not the text the file now holds.
        assert!(
            stale.spans[1].content.starts_with("pub struct Nothing"),
            "{stale:?}"
        );
    }

    /// What the border says instead: the selected row's file, the figure's own
    /// state, and how much of the workspace the figure does not describe.
    /// Pinned, because a stale list drawn as though it were current is a number
    /// acted on by mistake — and because the file is the one thing a row cannot
    /// carry, the pane being narrower than a path.
    #[test]
    fn the_risk_border_says_what_the_figure_is() {
        let mut state = measured();
        assert_eq!(risk_title(&state, 40), "risk  src/keys.rs");
        varde::risk::went_stale(&mut state.risk);
        assert_eq!(risk_title(&state, 40), "risk  src/keys.rs  stale");
        if let Figure::Stale(figures) = &mut state.risk.figure {
            figures.unparsed = 3;
        }
        assert_eq!(
            risk_title(&state, 45),
            "risk  src/keys.rs  stale  ·  3 unparsed"
        );
        // And at the width the pane actually opens at, cut to the columns the
        // action icons leave rather than drawn over them.
        assert_eq!(risk_title(&state, 30), "risk  src/keys.rs  stal…");
        state.risk.figure = Figure::None;
        // No figure and no job asked for is not a job: only a job in flight
        // says measuring.
        assert_eq!(risk_title(&state, 30), "risk  nothing measured");
        assert!(risk_lines(&state, 30).is_empty(), "a list with no figure");
        let _ = varde::risk::analyse(&mut state, varde::risk::Scope::Workspace);
        assert_eq!(risk_title(&state, 30), "risk  measuring");
    }

    /// The border and the rows agree about which row is selected: the file on
    /// the border is the file of the row the marker is on, since the row itself
    /// cannot carry it.
    #[test]
    fn the_border_names_the_file_of_the_row_the_marker_is_on() {
        let mut state = measured();
        state.risk_selection = 1;
        assert_eq!(risk_title(&state, 30), "risk  src/ui.rs");
        let lines = risk_lines(&state, 30);
        // The name and the figure, not the action icons after them: those carry
        // their own colour on the selected row, exactly as the tree's do.
        let marked = |line: &Line<'static>| {
            line.spans[..2]
                .iter()
                .all(|span| span.style.add_modifier.contains(Modifier::REVERSED))
        };
        assert!(!marked(&lines[0]), "an unselected row is marked");
        assert!(marked(&lines[1]), "the selected row is not marked");
        // A selection the figure has outgrown names nothing rather than the
        // wrong row: an analysis replaces the list wholesale.
        state.risk_selection = 9;
        assert_eq!(risk_title(&state, 30), "risk");
        assert!(risk_lines(&state, 30).iter().all(|line| !marked(line)));
    }

    /// One line changed in a two-line file, so the diff holds one of each kind
    /// of row: a context line, the line as it was, and the line as it is. The
    /// two sides disagree about line 2 on purpose — that is the only way a row
    /// read from the wrong side is visible at all.
    fn diffed() -> (State, Vec<DiffLine>) {
        let row = |new_line, old_line, removed, text: &str| DiffLine {
            new_line,
            old_line,
            removed,
            text: text.to_string(),
        };
        let diff = vec![
            row(Some(1), Some(1), false, "const kept = 1;"),
            row(None, Some(2), true, "const gone = \"old\";"),
            row(Some(2), None, false, "const gone = 2;"),
        ];
        let state = varde::update(
            &State::default(),
            Event::ShowDiff {
                file: "a.ts".to_string(),
                lines: diff.clone(),
                revision: String::new(),
            },
        )
        .0;
        (state, diff)
    }

    /// [`diffed`]'s rows, with both sides highlighted whole the way the edge
    /// hands them over.
    fn drawn(state: &State, diff: &[DiffLine]) -> Vec<Line<'static>> {
        let new = highlight::highlight("a.ts", "const kept = 1;\nconst gone = 2;");
        let old = highlight::highlight("a.ts", "const kept = 1;\nconst gone = \"old\";");
        diff_rows(state, diff, "a.ts", &new, &old)
    }

    /// The whole risk of spending foreground on the language: a reviewer who
    /// cannot tell an addition from context is worse off than one reading grey
    /// code. So the marker column and the row's background have to carry the
    /// row's kind on their own, and no two kinds may spell that the same way.
    #[test]
    fn the_three_kinds_of_diff_row_are_told_apart_without_the_foreground() {
        let (state, diff) = diffed();
        let rows = drawn(&state, &diff);
        let markers: Vec<&Span<'static>> = rows.iter().map(|row| &row.spans[1]).collect();
        assert_eq!(
            markers
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<Vec<_>>(),
            [" ", "-", "+"].map(|marker| format!("{marker} ")),
        );
        let own: Vec<(Option<Color>, Option<Color>)> = markers
            .iter()
            .map(|span| (span.style.fg, span.style.bg))
            .collect();
        for (index, kind) in own.iter().enumerate() {
            assert_eq!(
                own.iter().filter(|other| *other == kind).count(),
                1,
                "row {index} is spelt like another kind: {own:?}"
            );
        }
    }

    /// The point of the whole change: a diff row's code carries its language's
    /// colour, and each row takes it from its own side. Line 2 is a number in
    /// the new file and a string in the old one, so a removed row coloured from
    /// the new side would come back yellow.
    #[test]
    fn a_diff_row_is_coloured_by_its_own_sides_language() {
        let (state, diff) = diffed();
        let rows = drawn(&state, &diff);
        // Past the gutter: a line number trims to the same text a number in the
        // code does, and the gutter is grey by design.
        let kind = |row: &Line<'static>, text: &str| {
            row.spans[2..]
                .iter()
                .find(|span| span.content.trim() == text)
                .unwrap_or_else(|| panic!("no {text:?} in {row:?}"))
                .style
                .fg
        };
        assert_eq!(kind(&rows[0], "const"), Some(colour(Kind::Keyword, true)));
        assert_eq!(kind(&rows[1], "\"old\""), Some(colour(Kind::String, true)));
        assert_eq!(kind(&rows[2], "2"), Some(colour(Kind::Number, true)));
        // A side that could not be read leaves the row flat, at the colour its
        // own marker carries — never the colour of the side that could.
        let flat = diff_rows(&state, &diff, "a.ts", &[], &[]);
        for (index, row) in flat.iter().enumerate() {
            assert_eq!(row.spans.len(), 3, "{row:?}");
            assert_eq!(row.spans[2].style.fg, row.spans[1].style.fg, "row {index}");
        }
    }

    /// A comment is a row of the diff, counted by the scroll clamp and the
    /// mouse hit-test — so where it sits is load-bearing, and the loop that
    /// interleaves them was rebuilt underneath it. It is not a line of code
    /// either: the row's tint would read as a comment on the *other* side.
    #[test]
    fn a_comment_keeps_its_own_row_and_its_own_colour() {
        let (mut state, diff) = diffed();
        state.comments = vec![varde::review::Comment {
            file: "a.ts".to_string(),
            from_line: 2,
            to_line: 2,
            kind: "ISSUE".to_string(),
            body: "still unquoted".to_string(),
            revision: String::new(),
            story: None,
            step: None,
        }];
        let rows = drawn(&state, &diff);
        // Under the row whose new-file line it covers — the added one, not the
        // removed line above it that carries the same text.
        assert_eq!(rows.len(), diff.len() + 1);
        assert!(rows[3].to_string().contains("ISSUE still unquoted"));
        assert_eq!(rows[3].spans[0].style.fg, Some(Color::Red));
        assert_eq!(
            rows[3].spans[0].style.bg, None,
            "a comment took a row's tint"
        );
    }

    /// A slid Preview shows the tail of a code fence, which is the case the
    /// gesture exists for: a fence is laid out one row per source line and
    /// never wrapped, so the only way past the pane's right edge is the offset.
    /// Rendered rather than read off the return value, because a `Paragraph`
    /// will not give its lines back — the same reason the pane-action test
    /// renders.
    #[test]
    fn a_slid_preview_draws_the_tail_of_a_wide_row() {
        use ratatui::widgets::Widget;
        let rows = varde::preview::rows("```rust\nabcdefghijklmnop\n```\n", 8);
        let mut state = State::default();
        state.editor_hscroll = 6;
        let area = ratatui::layout::Rect::new(0, 0, 12, 4);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        super::preview_widget(&state, None, Line::from("README.md"), Vec::new(), &rows, 80)
            .render(area, &mut buffer);
        let drawn: String = (0..12)
            .map(|column| buffer[(column, 1)].symbol().to_string())
            .collect();
        // Inside the block's left border, so column 1 of the pane is the first
        // character drawn. A Preview has no gutter to hold back.
        assert!(
            drawn.contains("ghijklmnop"),
            "the fence did not slide: {drawn:?}"
        );
    }

    /// A picked span of a Preview is drawn reversed, the way Source's is. The
    /// bug this guards is a drag that sets a selection nothing paints: it
    /// reads as selection being broken in a Preview, and `:read` unreachable
    /// with it.
    #[test]
    fn a_picked_span_of_a_preview_is_drawn_reversed() {
        use ratatui::widgets::Widget;
        let rows = varde::preview::rows("one two\n", 20);
        let mut state = State::default();
        state.selection = Some(varde::Selection::Screen {
            pane: varde::Pane::Editor,
            from: varde::Place { line: 1, column: 1 },
            to: varde::Place { line: 1, column: 3 },
            text: "one".to_string(),
        });
        let area = ratatui::layout::Rect::new(0, 0, 12, 3);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        super::preview_widget(&state, None, Line::from("README.md"), Vec::new(), &rows, 80)
            .render(area, &mut buffer);
        let reversed = |column: u16| {
            buffer[(column, 1)]
                .style()
                .add_modifier
                .contains(Modifier::REVERSED)
        };
        assert!(reversed(1) && reversed(3), "the span was not marked");
        assert!(!reversed(4), "the space after it was marked too");
    }

    /// A slid diff keeps the two spans that say which row this is — the number
    /// and the `+`/`-` — and slides only the code past them. A comment is about
    /// a line rather than a column of it, so it stays where it is: slid with
    /// the code, a comment on a wide line would be blank at exactly the offset
    /// the reader needed it at.
    #[test]
    fn sliding_a_diff_leaves_its_numbers_markers_and_comments_alone() {
        let (mut state, diff) = diffed();
        state.comments = vec![varde::review::Comment {
            file: "a.ts".to_string(),
            from_line: 2,
            to_line: 2,
            kind: "ISSUE".to_string(),
            body: "still unquoted".to_string(),
            revision: String::new(),
            story: None,
            step: None,
        }];
        let home = drawn(&state, &diff);
        state.editor_hscroll = 6;
        let slid = drawn(&state, &diff);
        assert_eq!(slid[0].spans[0], home[0].spans[0], "the number moved");
        assert_eq!(slid[0].spans[1], home[0].spans[1], "the marker moved");
        let code = |row: &Line<'static>| {
            row.spans[2..]
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        };
        assert_eq!(code(&slid[0]), "kept = 1;");
        assert_eq!(slid[3], home[3], "the comment slid with the code");
    }

    /// Selecting rows to comment on is the diff's one interaction, and the tint
    /// the language's colours are drawn over must not swallow it.
    #[test]
    fn a_selected_diff_row_is_still_marked_once_its_code_is_coloured() {
        let (state, diff) = diffed();
        let selected = varde::update(&state, Event::EditorKey('V')).0;
        let rows = drawn(&selected, &diff);
        let marked = |row: &Line<'static>| {
            row.spans[1..]
                .iter()
                .all(|span| span.style.add_modifier.contains(Modifier::REVERSED))
        };
        assert!(marked(&rows[0]), "the selected row is not marked");
        assert!(!marked(&rows[1]), "an unselected row is marked");
    }

    fn one_file_tree() -> (State, Vec<Row>) {
        let mut state = State::default();
        state.root = std::path::PathBuf::from("/w");
        state.contents.insert(
            state.root.clone(),
            vec![varde::tree::Entry {
                name: "main.rs".to_string(),
                is_dir: false,
            }],
        );
        let rows = varde::tree::rows(&state);
        (state, rows)
    }

    /// The glyph carries the colour and the filename is left alone — colouring
    /// the name too turns the pane into a rainbow and costs the dimmed and
    /// selected styles their meaning. Asserted on the spans, since a
    /// `Paragraph` will not give its text back.
    #[test]
    fn only_the_glyph_is_coloured_and_the_name_is_left_alone() {
        let (state, rows) = one_file_tree();
        let line = tree_lines(&state, &rows, 40).remove(0);
        let coloured: Vec<&str> = line
            .spans
            .iter()
            .filter(|span| span.style.fg == Some(icon_colour(IconKind::Rust)))
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(coloured, ["\u{e7a8}"], "the Rust glyph, and nothing else");
        let name = line
            .spans
            .iter()
            .find(|span| span.content.contains("main.rs"))
            .expect("the filename");
        assert_eq!(name.style.fg, None, "the name stays uncoloured");
    }

    /// `mouse::action_at` hit-tests the action icons from the pane's right
    /// edge, so the label before them has to fill exactly the width the tree
    /// reserved. The padding used to ride on one label span; making the glyph
    /// its own span moved it onto the name, and getting that wrong lands every
    /// click on the tree's icons one column off.
    #[test]
    fn the_label_fills_the_columns_the_actions_are_hit_tested_from() {
        let (mut state, rows) = one_file_tree();
        // Actions are offered on the selected row only, so that is the row the
        // padded branch draws.
        state.tree_selection = Some(rows[0].path.clone());
        let width = 40;
        let line = tree_lines(&state, &rows, width).remove(0);
        assert_eq!(
            varde::tree::row_actions(&state, &rows[0].path).len(),
            2,
            "a file offers delete and copy-path, so this is the padded branch"
        );
        // width - 3 for the borders and the mark column, less two columns per
        // action — the same arithmetic tree_lines does.
        let label: usize = line.spans[1..4]
            .iter()
            .map(|span| span.content.chars().count())
            .sum();
        assert_eq!(label, (width as usize - 3) - 4);
    }

    /// Colours picked by hand, and two that happen to be equal make two
    /// languages indistinguishable with nothing to say so. The match in
    /// `icon_colour` is exhaustive, so a new kind is a compile error; this is
    /// the half the compiler cannot check. It sweeps `IconKind::ALL` rather
    /// than a list of its own, because a list of its own is a list that stops
    /// covering the kinds added after it — which is the exact blind spot this
    /// test exists to close.
    #[test]
    fn no_two_icon_kinds_share_a_colour() {
        let kinds = IconKind::ALL;
        for (index, kind) in kinds.iter().enumerate() {
            for other in &kinds[index + 1..] {
                assert_ne!(
                    icon_colour(*kind),
                    icon_colour(*other),
                    "{kind:?} and {other:?}"
                );
            }
        }
    }

    /// The suite asserts the title through `varde::mode_label`, which is the
    /// part of it the core owns. This is what holds the drawn title to carrying
    /// that clause — without it the scenario could go green over a title that
    /// says nothing about which of the two shapes is on screen.
    #[test]
    fn the_title_carries_the_mode_the_core_named() {
        let buffer = varde::editor::Buffer::open("# Setup", true, 4);
        let title = buffer_title("README.md", &buffer, "preview", 80);
        let drawn: String = title
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(drawn.contains("README.md"), "{drawn:?}");
        assert!(drawn.contains("preview"), "{drawn:?}");
    }

    /// The title carries three things at once and only two of them are
    /// warnings. Pinned by colour rather than by wording: the words are free to
    /// change, but a dirty dot and a divergence drawn in the same colour as the
    /// name are what made both easy to miss on a screen an AI is writing to.
    #[test]
    fn the_title_colours_the_marks_and_leaves_the_name_alone() {
        let mut buffer = varde::editor::Buffer::open("on disk", false, 4);
        buffer.key('x');
        buffer.follow("changed underneath".to_string());
        let title = buffer_title("test.md", &buffer, "normal", 80);
        let coloured = |fg| {
            title
                .spans
                .iter()
                .filter(|span| span.style.fg == Some(fg))
                .map(|span| span.content.to_string())
                .collect::<String>()
        };
        assert_eq!(coloured(DIRTY), " ●", "unsaved work");
        assert!(coloured(WARNING).contains("diverged from disk"), "the ⚠");
        assert_eq!(title.spans[0].content, "test.md", "the name");
        assert_eq!(title.spans[0].style.fg, None, "and it stays uncoloured");
    }

    /// A clean buffer that agrees with disk wears neither mark, so nothing in
    /// the title is coloured at all.
    #[test]
    fn a_clean_buffer_has_an_unmarked_title() {
        let title = buffer_title(
            "test.md",
            &varde::editor::Buffer::open("x", false, 4),
            "normal",
            80,
        );
        assert!(title.spans.iter().all(|span| span.style.fg.is_none()));
    }

    /// A blank line inside a block has nothing to substitute a glyph into, so
    /// its guides are laid down past the end of it. Dropping them is what left
    /// a guide broken over every empty line in a function.
    ///
    /// What is laid down between them is blank and not dotted, because a dot
    /// stands for a space somebody typed and an empty line holds none.
    #[test]
    fn a_blank_line_is_given_the_spaces_its_guides_need() {
        let line = Line::from(vec![Span::raw("   3 "), Span::raw("")]);
        let guides = [
            varde::editor::Guide {
                column: 0,
                active: false,
            },
            varde::editor::Guide {
                column: 4,
                active: true,
            },
        ];
        let drawn = guided(&line, &guides, &[], Style::default(), faint(None));
        let text: String = drawn.spans.iter().map(|span| &*span.content).collect();
        assert_eq!(
            text, "   3 \u{2502}   \u{2503}",
            "the padding is blank, not dotted: there are no spaces on this line to draw"
        );
    }

    /// An indented line keeps every column where it was: a glyph replaces the
    /// space at the guide, never sits beside it.
    #[test]
    fn a_guide_replaces_the_space_it_stands_in() {
        let line = Line::from(vec![Span::raw("   3 "), Span::raw("        foo()")]);
        let guides = [varde::editor::Guide {
            column: 4,
            active: false,
        }];
        let drawn = guided(
            &line,
            &guides,
            &[11, 12],
            Style::default().bg(Color::Indexed(236)),
            faint(None),
        );
        let text: String = drawn.spans.iter().map(|span| &*span.content).collect();
        assert_eq!(
            text, "   3 \u{b7}\u{b7}\u{b7}\u{b7}\u{2502}\u{b7}\u{b7}\u{b7}foo()",
            "a guide takes its own column and every other space is a dot"
        );
        assert_eq!(
            drawn.spans[12].style.bg,
            Some(Color::Indexed(236)),
            "the bracket is not marked"
        );
    }

    /// A dot and a guide are one tier. Two palette slots are how the dot went
    /// darker than a Gruvbox background while the guide beside it did not.
    #[test]
    fn a_dot_and_a_guide_are_one_tier() {
        let line = Line::from(vec![Span::raw("   3 "), Span::raw("   x")]);
        let guides = [
            varde::editor::Guide {
                column: 0,
                active: false,
            },
            varde::editor::Guide {
                column: 2,
                active: true,
            },
        ];
        let faint = faint(Some([[235, 219, 178], [40, 40, 40]]));
        let drawn = guided(&line, &guides, &[], Style::default(), faint);
        assert_eq!(drawn.spans[1].style, faint, "the guide");
        assert_eq!(drawn.spans[2].style, faint, "the dot");
        assert_eq!(drawn.spans[3].style, faint, "the cursor's guide");
    }

    /// Nearer the page than the text, on either kind of page: DIM, the
    /// terminal's own half-way fade, read as competing with the code.
    #[test]
    fn the_faint_layer_is_mixed_nearer_the_page_than_the_text() {
        assert_eq!(
            faint(Some([[235, 219, 178], [40, 40, 40]])),
            Style::new().fg(Color::Rgb(63, 61, 56)),
            "a dark page"
        );
        assert_eq!(
            faint(Some([[40, 40, 40], [250, 250, 250]])),
            Style::new().fg(Color::Rgb(224, 224, 224)),
            "a light page"
        );
        assert_eq!(
            faint(None),
            Style::new().fg(Color::Reset).add_modifier(Modifier::DIM),
            "a terminal that would not say its colours"
        );
    }

    fn row() -> Line<'static> {
        Line::from(vec![
            Span::raw("   7 "),
            Span::styled("let ", Style::default().fg(Color::Magenta)),
            Span::raw("widest = 1;"),
        ])
        .style(Style::default().add_modifier(Modifier::REVERSED))
    }

    #[test]
    fn shifting_slides_the_text_and_leaves_the_line_number() {
        let mut state = State::default();
        state.editor_hscroll = 6;
        let mut lines = [row()];
        shift(&mut lines, &state, 1);
        let text: String = lines[0].spans.iter().map(|span| &*span.content).collect();
        assert_eq!(text, "   7 dest = 1;");
        assert_eq!(lines[0].style, row().style, "the selection is on the line");
    }

    /// A shift that lands inside the first text span keeps the rest of it, and
    /// keeps its colour: syntax highlighting does not stop at the left edge.
    #[test]
    fn shifting_into_a_span_keeps_its_style() {
        let mut state = State::default();
        state.editor_hscroll = 2;
        let mut lines = [row()];
        shift(&mut lines, &state, 1);
        assert_eq!(lines[0].spans[1].content, "t ");
        assert_eq!(lines[0].spans[1].style.fg, Some(Color::Magenta));
    }

    /// Past the end of a line there is nothing to draw, and a blank row is the
    /// honest answer — not the line's tail pulled back into view.
    #[test]
    fn shifting_past_the_end_leaves_only_the_line_number() {
        let mut state = State::default();
        state.editor_hscroll = 40;
        let mut lines = [row()];
        shift(&mut lines, &state, 1);
        assert_eq!(lines[0].spans.len(), 1);
        assert_eq!(lines[0].spans[0].content, "   7 ");
    }

    #[test]
    fn a_name_that_fits_is_left_unchanged() {
        assert_eq!(truncate("First", 20), "First");
    }

    #[test]
    fn an_over_length_name_truncates_with_an_ellipsis() {
        assert_eq!(truncate("A very long step name indeed", 10), "A very lo…");
    }

    /// Cut by display width, not by `chars().count()`: a wide glyph split on a
    /// character boundary must not overrun the column it was cut to fit.
    #[test]
    fn a_wide_glyph_is_not_split_across_the_cut() {
        // Each 'あ' is two columns wide, so a width of 5 fits two of them (4)
        // and has room for the ellipsis, not a third that would overrun it.
        assert_eq!(truncate("ああああ", 5), "ああ…");
    }

    /// Which colour is a theme's business, so nothing pins one. That the kinds
    /// are told *apart* is the whole point of widening the vocabulary — two
    /// arms quietly collapsing to one colour is a kind nobody can see, and no
    /// scenario may assert a colour.
    #[test]
    fn no_two_kinds_share_a_colour() {
        let kinds = [
            Kind::Keyword,
            Kind::Operator,
            Kind::String,
            Kind::Comment,
            Kind::Number,
            Kind::Constant,
            Kind::Function,
            Kind::Type,
            Kind::Property,
            Kind::Attribute,
            Kind::Punctuation,
            Kind::Markup,
            Kind::Invalid,
            Kind::Plain,
        ];
        for dark in [true, false] {
            for (index, one) in kinds.iter().enumerate() {
                for other in &kinds[index + 1..] {
                    assert_ne!(
                        colour(*one, dark),
                        colour(*other, dark),
                        "{one:?} and {other:?} at dark={dark}"
                    );
                }
            }
        }
    }

    /// What a fold costs the drawn lines: the body is gone, the line that
    /// opens the block carries its toggle in `layout::TOGGLE_COLUMN` and, once
    /// folded, the dots standing for what is hidden — so the text still starts
    /// where `layout::GUTTER` says it does and a drag lands on the character
    /// it is over.
    #[test]
    fn a_folded_block_leaves_its_opening_line_carrying_a_toggle() {
        let source = "fn main() {\n    go();\n}\n";
        let path = std::path::PathBuf::from("/w/src/main.rs");
        let mut state = State::default();
        state
            .buffers
            .insert(path.clone(), varde::editor::Buffer::open(source, false, 4));
        state.current_buffer = Some(path.clone());
        let tokens = highlight::highlight("main.rs", source);
        let drawn = |state: &State| {
            source_lines(
                state,
                &path,
                &state.buffers[&path],
                (&tokens, &[]),
                ratatui::layout::Rect::new(0, 0, 40, 10),
                faint(None),
            )
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<String>>()
        };
        assert_eq!(
            drawn(&state),
            [
                "    1 \u{25bc}  fn\u{b7}main()\u{b7}{",
                "    2    \u{2503}\u{b7}\u{b7}\u{b7}go();",
                "    3    }",
                "    4    "
            ]
        );
        assert_eq!(
            drawn(&state)[0].chars().nth(layout::TOGGLE_COLUMN as usize),
            Some('\u{25bc}'),
            "drawn in the column `mouse` hit-tests it at"
        );
        assert!(
            drawn(&state)
                .iter()
                .all(|line| line.chars().count() >= layout::GUTTER as usize),
            "and the gutter is `layout::GUTTER` columns wide on every line"
        );

        varde::fold::toggle(state.buffers.get_mut(&path).expect("the buffer"), false);
        assert_eq!(
            drawn(&state),
            [
                format!("    1 \u{25ba}  fn\u{b7}main()\u{b7}{{{DOTS}"),
                "    3    }".to_string(),
                "    4    ".to_string()
            ]
        );
    }

    /// A Breakpoint takes the gutter's first column, the one `mouse` hit-tests
    /// the click that set it at, and moves nothing else on the line: the
    /// number, the toggle and the text are where they were.
    #[test]
    fn a_breakpoint_is_drawn_in_the_column_its_click_lands_in() {
        let plain = code_lines(
            &highlight::highlight("main.rs", "fn main() {}"),
            [1],
            true,
            None,
        )
        .remove(0);
        let text = |line: &Line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        };
        let marked = text(&with_breakpoint(plain.clone(), varde::debug::Mark::Plain));
        assert_eq!(
            marked.chars().nth(layout::BREAKPOINT_COLUMN as usize),
            Some('\u{25cf}')
        );
        assert_eq!(
            marked.chars().skip(1).collect::<String>(),
            text(&plain).chars().skip(1).collect::<String>()
        );
        assert_eq!(
            text(&with_breakpoint(plain.clone(), varde::debug::Mark::Stale))
                .chars()
                .next(),
            Some('\u{25cc}'),
            "a Stale breakpoint is drawn apart from one the program will pause at"
        );
        assert_eq!(
            text(&with_breakpoint(plain, varde::debug::Mark::Unverified))
                .chars()
                .next(),
            Some('\u{25cb}'),
            "an Unverified breakpoint is hollow, and drawn apart from a Stale one"
        );
    }

    /// Every occurrence the next-occurrence gesture took, drawn picked the way
    /// the selection itself is — the whole of what makes the gesture visible,
    /// since a word that will be typed over and shows nothing reads as the key
    /// having done nothing at all.
    #[test]
    fn every_occurrence_taken_is_drawn_picked_the_way_the_selection_is() {
        let mut state = State::default();
        state.selection = Some(Selection::Buffer {
            anchor: Place { line: 1, column: 1 },
            cursor: Place { line: 1, column: 3 },
        });
        state.occurrences = vec![Place { line: 1, column: 9 }];
        let mut lines = vec![Line::from(vec![Span::raw("1 "), Span::raw("one and one")])];
        paint_drag(
            &mut lines,
            &|number| Some(number - 1),
            state.selection.as_ref().and_then(Selection::buffer_span),
            &state.occurrences,
            true,
        );
        // Bracketed rather than collected: what is asserted is *which* columns
        // are picked, and the spans are cut per character.
        let mut drawn = String::new();
        let mut picked = false;
        for span in &lines[0].spans {
            let reversed = span.style.add_modifier.contains(Modifier::REVERSED);
            if reversed != picked {
                drawn.push('|');
                picked = reversed;
            }
            drawn.push_str(&span.content);
        }
        assert_eq!(drawn, "1 |one| and |one");
    }

    /// The comment box's caret, which is the one piece of arithmetic this file
    /// does. The character under the caret is kept and reversed rather than
    /// covered, and the line is cut by characters, because a byte offset lands
    /// inside the first multi-byte glyph somebody types.
    #[test]
    fn the_caret_reverses_the_character_it_sits_on_and_hides_nothing() {
        let drawn = |line: &str, column| {
            let spans = with_caret(line, column);
            assert!(
                spans[1].style.add_modifier.contains(Modifier::REVERSED),
                "the caret is the reversed span"
            );
            format!(
                "{}[{}]{}",
                spans[0].content, spans[1].content, spans[2].content
            )
        };
        assert_eq!(drawn("abc", 1), "[a]bc");
        assert_eq!(drawn("abc", 2), "a[b]c");
        // Past the last character, which is where insert mode leaves it: the
        // caret covers a space it invents rather than a character it hides.
        assert_eq!(drawn("abc", 4), "abc[ ]");
        assert_eq!(drawn("", 1), "[ ]");
        // Multi-byte, where a byte offset would cut inside a glyph.
        assert_eq!(drawn("nåværende", 3), "nå[v]ærende");
        // A column past the end never panics and never wraps.
        assert_eq!(drawn("ab", 99), "ab[ ]");
    }

    /// The key box truncates from the bottom, so `keys::CHEATSHEET`'s order is
    /// what decides which rows exist on a short screen — and 26 rows by 120
    /// columns leaves Edit view's twenty-five rows competing for sixteen. It is
    /// the size the replay recipe in `AGENTS.md` uses and the size
    /// `features/ai_pane.feature` drives, and it is where `:format` — the whole
    /// of what makes the formatter discoverable, since it is in no palette, has
    /// no completion and is spelled nowhere else — was drawn off the bottom
    /// along with the gesture that opens the palette.
    ///
    /// Not pinned at every row the table claims: at this size that assertion
    /// cannot pass, and rows are excused off the bottom on purpose. What is
    /// held is the two rows nothing else in Varde teaches. `C-f`, `D` and
    /// `:w :q` are what they displaced, each of which is said again somewhere
    /// the reader is already looking: the palette lists `(f) Find`, and the
    /// `buffer-diverged` and `unsaved-changes` notices name `D`, `:w` and `:q!`
    /// in the sentence that reports the problem they answer.
    ///
    /// Not pinned at 100 columns either, which is the other size in the suite:
    /// the editor pane is 42 wide there, the width guard returns before drawing
    /// anything, and an assertion about rows in a box nobody drew cannot fail.
    #[test]
    fn the_rows_nothing_else_teaches_survive_a_short_window() {
        let mut state = State::default();
        state.cheatsheet = true;
        let editor =
            varde::layout::panes(120, 26, 30, None, 0, 0, varde::layout::Shapes::default()).editor;
        assert_eq!(editor.height, 18, "the pane the box is drawn in");

        let drawn: Vec<String> = cheatsheet_rows(&state, editor.height)
            .into_iter()
            .map(|(row, _)| row)
            .collect();
        assert_eq!(drawn.len(), 16);
        for keys in ["C-space Esc Esc", ":format"] {
            assert!(
                drawn.iter().any(|row| row.trim_start().starts_with(keys)),
                "{keys:?} is drawn off the bottom at 26 rows: {drawn:?}"
            );
        }
    }

    fn text(line: Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    /// The version tag ends the bottom row whatever the notice is: a notice too
    /// long for the row is cut before the tag rather than drawn over it.
    #[test]
    fn the_version_tag_is_right_aligned_and_a_long_notice_is_cut_before_it() {
        let short = text(status_line("saved", Tone::Notice, "0.2.0", None, 40));
        assert_eq!(short.width(), 40);
        assert!(short.starts_with("saved "), "{short:?}");
        assert!(short.ends_with(" v0.2.0"), "{short:?}");

        let long = "x".repeat(100);
        let cut = text(status_line(&long, Tone::Notice, "0.2.0", Some("0.3.0"), 80));
        assert_eq!(cut.width(), 80);
        assert!(
            cut.ends_with("x… v0.2.0 → v0.3.0  C-space u to update"),
            "{cut:?}"
        );
    }

    /// A row too narrow for the notice and the whole tag drops the tag's hint
    /// before it drops the tag, and the tag before it drops the notice.
    #[test]
    fn a_narrow_row_drops_the_hint_then_the_tag() {
        let at = |width| {
            text(status_line(
                "saved",
                Tone::Notice,
                "0.2.0",
                Some("0.3.0"),
                width,
            ))
        };
        assert!(at(80).ends_with(" v0.2.0 → v0.3.0  C-space u to update"));
        assert!(at(40).ends_with(" v0.2.0 → v0.3.0"), "{:?}", at(40));
        assert_eq!(at(20), "saved");
    }

    /// A newer Version is a manifest's or a network response's, so it is text
    /// from outside and is drawn without its control characters.
    #[test]
    fn the_version_tag_draws_no_control_characters() {
        let drawn = text(status_line(
            "",
            Tone::Hint,
            "0.2.0",
            Some("0.3.0\x1b[2J"),
            80,
        ));
        assert!(!drawn.contains('\x1b'), "{drawn:?}");
    }

    /// A thematic break is the one Preview row whose glyph is not in its text:
    /// it carries no pieces, so it drew as a blank line and `row_style`'s
    /// `Rule` arm had a colour with nothing to colour. `.scratch`'s ticket 03
    /// asks for "a rule across the pane" and the spec for "nothing renders as
    /// nothing"; what shipped was three blank rows per `---`. It is a server's
    /// markdown too, so a `rust-analyzer` hover — which divides every section
    /// with `---` — spent six of its sixteen rows on nothing at all.
    ///
    /// Held on the drawn line and not on the row, deliberately: the glyph must
    /// not be in `Row::text`, which the hover's own width is measured off and
    /// which find-in-file, the word motions and drag-copy all read.
    #[test]
    fn a_thematic_break_draws_as_a_rule_and_not_as_a_blank() {
        let rows = varde::preview::rows("one\n\n---\n\ntwo\n", 40);
        let rule = rows
            .iter()
            .find(|row| row.kind == varde::preview::RowKind::Rule)
            .expect("a Rule row");
        assert_eq!(rule.text(), "", "the glyph must stay out of the text");

        let drawn: String = preview_line(rule, true, 12)
            .spans
            .iter()
            .map(|span| span.content.as_ref().to_string())
            .collect();
        assert_eq!(drawn, "────────────");

        // And every other kind still draws its own pieces: the arm above is a
        // rule for one kind, not a rule for empty rows.
        let prose = rows
            .iter()
            .find(|row| row.text() == "one")
            .expect("the paragraph");
        let text: String = preview_line(prose, true, 12)
            .spans
            .iter()
            .map(|span| span.content.as_ref().to_string())
            .collect();
        assert_eq!(text, "one");
    }

    /// `.scratch` ticket: a numbered list in `docs/todo.md` previewed with no
    /// numbers at all. The core keeps the markup out of `Row::text` on purpose
    /// (`preview::MARKERS`), so the bullet, the ordinal and the checkbox are
    /// the theme's to draw — and nothing drew them, which turned every list
    /// into unindented prose.
    #[test]
    fn a_list_row_draws_its_marker_and_indents_to_its_depth() {
        let drawn = |row| {
            preview_line(row, true, 40)
                .spans
                .iter()
                .map(|span| span.content.as_ref().to_string())
                .collect::<String>()
        };
        let rows = varde::preview::rows(
            "1. first\n2. second\n   - nested\n\n- [ ] todo\n- [x] done\n",
            40,
        );
        let listed: Vec<String> = rows
            .iter()
            .filter(|row| matches!(row.kind, varde::preview::RowKind::List(_)))
            .map(drawn)
            .collect();
        assert_eq!(
            listed,
            [
                "1. first",
                "2. second",
                "  \u{2022} nested",
                "\u{2610} todo",
                "\u{2611} done",
            ]
        );
    }
}
