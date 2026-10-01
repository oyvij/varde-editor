use crate::pty::Pane as PtyPane;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use std::collections::HashMap;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use varde::highlight::{self, Kind};
use varde::layout::{self, Area};
use varde::lsp;
use varde::minimap;
use varde::tree::Row;
use varde::{
    filter, keys, mark, palette_rows, reading, review, story, tools, tree, Mark, Modal, Pane,
    Place, Selection, State, View,
};

const DIRTY: Color = Color::LightYellow;

const WARNING: Color = Color::Indexed(208);
const READING: Color = Color::Cyan;
const FAINT: Style = Style::new().fg(Color::Reset).add_modifier(Modifier::DIM);
const FAINT_INK: u16 = 12;

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

#[derive(Clone, Copy)]
pub enum Tone {
    Hint,
    Notice,
    Warning,
}

pub struct Chrome<'a> {
    pub status: &'a str,
    pub tone: Tone,
    pub name_draft: &'a str,
    pub command_draft: Option<&'a str>,
    pub ai_draft: &'a str,
    pub comment_kind: &'a str,
    pub filter_draft: Option<&'a str>,
    pub tokens: &'a [Vec<highlight::Token>],
    pub run_marks: &'a [usize],
    pub diff_new: &'a [Vec<highlight::Token>],
    pub diff_old: &'a [Vec<highlight::Token>],
    pub preview: &'a [varde::preview::Row],
    pub code: &'a Code,
    pub faint: Style,
}

pub type Code = HashMap<String, Vec<Vec<highlight::Token>>>;

pub struct Areas {
    pub tree: Rect,
    pub editor: Rect,
    pub ai: Rect,
    pub splits: Vec<Rect>,
    pub band: Rect,
    pub step_menu: Rect,
    pub corner: Rect,
    pub minimap: Rect,
    pub panes: layout::Layout,
}

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
    let typing = chrome.command_draft.map(|draft| format!(":{draft}█"));
    let finding = state
        .find
        .as_ref()
        .is_some_and(|find| find.keys != varde::FindKeys::Away);
    // Painted on the inner area: Block::style would also paint the border ring
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
    minimap(frame, state, &areas, chrome.tokens);
    replace_box(frame, state, areas.panes.editor);
    if !finding {
        place_cursor(frame, state, &areas, typing.as_deref());
    }
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
                variables_widget(
                    state,
                    areas.panes.terminal.width,
                    chrome.name_draft,
                    chrome.code,
                ),
                rect(areas.panes.terminal),
            );
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
    match state.ai_slot {
        layout::Slot::Ai => draw_ai_pane(frame, state, &areas, ai, &chrome, caret_is_free),
        layout::Slot::Cheatsheet => frame.render_widget(cheatsheet_widget(state), areas.ai),
    }
    draw_status(frame, state, &chrome);
    if let Some((x, y)) = varde::debug::edit_chip(state, &areas.panes) {
        frame.render_widget(
            Paragraph::new(action_icon(varde::debug::EDIT)),
            Rect::new(x, y, 1, 1),
        );
    }

    hover(frame, state, &areas.panes);
    breakpoint_reason(frame, state, &areas.panes);
    diagnostic_box(frame, state, &areas.panes);
    candidates(frame, state, &areas.panes);
    evaluator(frame, state, &areas.panes, chrome.code);

    if state.search.is_some() {
        search_screen(frame, state);
        return;
    }
    draw_modal(frame, state, &chrome);
}

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

fn draw_modal(frame: &mut Frame, state: &State, chrome: &Chrome) {
    match &state.modal {
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
        Modal::Candidates(_) => {}
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

/// One Line per line: ratatui draws an embedded \n as a control character, not a break
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

fn risk_title(state: &State, width: u16) -> String {
    use varde::risk::Standing;
    // ratatui draws a left-aligned title over a right-aligned one, so cut it before the icons
    let room = (width as usize).saturating_sub(2 + 2 * varde::risk::pane_actions(state).len());
    let mut title = "risk".to_string();
    if state.refactor.running.is_some() {
        if let Some(status) = varde::risk::status(state) {
            title.push_str("  ");
            title.push_str(&status);
        }
        return truncate(&title, room);
    }
    if let Some(function) = varde::risk::selected(state) {
        title.push_str("  ");
        title.push_str(&function.file);
    }
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
    if let Some(status) = varde::risk::status(state) {
        title.push_str("  ·  ");
        title.push_str(&status);
    }
    truncate(&title, room)
}

fn risk_widget(state: &State, width: u16) -> Paragraph<'static> {
    Paragraph::new(risk_lines(state, width))
        .scroll((state.risk_scroll as u16, 0))
        .block(
            pane_block(risk_title(state, width), state, Pane::Risk)
                .title(pane_actions_title(state)),
        )
}

fn pane_actions_title(state: &State) -> Line<'static> {
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

fn action_style(state: &State, action: &str, armed: bool) -> Style {
    match armed || state.hovered_action == Some(action) {
        true => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        false => Style::default().fg(Color::DarkGray),
    }
}

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
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| varde::relative(state, path));
            let (mark, colour) = glyph(varde::mark(state, path));
            let room = inner.saturating_sub(2);
            Line::from(vec![
                Span::styled(mark, selected.fg(colour)),
                Span::styled(format!(" {}", truncate(&name, room)), selected),
            ])
        })
        .collect()
}

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

fn frames_widget(state: &State, width: u16) -> Paragraph<'static> {
    let widget = Paragraph::new(frames_lines(state, width))
        .scroll((state.frames_scroll as u16, 0))
        .block(pane_block("frames", state, Pane::Frames));
    dimmed_while_running(state, widget)
}

fn dimmed_while_running(state: &State, widget: Paragraph<'static>) -> Paragraph<'static> {
    match varde::debug::stale(state) {
        true => widget.style(Style::default().add_modifier(Modifier::DIM)),
        false => widget,
    }
}

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

fn variables_widget(state: &State, width: u16, draft: &str, code: &Code) -> Paragraph<'static> {
    let widget = Paragraph::new(variables_lines(state, width, draft, code))
        .scroll((state.variables_scroll as u16, 0))
        .block(pane_block(
            varde::debug::title(state),
            state,
            Pane::Variables,
        ));
    dimmed_while_running(state, widget)
}

fn variables_lines(state: &State, width: u16, draft: &str, code: &Code) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(2) as usize;
    let dark = state.editor_theme != "light";
    varde::debug::variables(state)
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let style = match index == state.variables_selection {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
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
                Span::styled(" ", style),
            ];
            let pad = room.saturating_sub(1);
            match (row.of, tokens_of(code, &row.value)) {
                (varde::debug::Of::Watch { failed: true, .. }, _) => spans.push(Span::styled(
                    format!("{:<pad$}", truncate(&row.value, pad)),
                    style.fg(WARNING),
                )),
                (_, Some(lines)) => spans.extend(cut_spans(&one_row(lines), pad, style, dark)),
                (_, None) => spans.push(Span::styled(
                    format!("{:<pad$}", truncate(&row.value, pad)),
                    style.fg(Color::DarkGray),
                )),
            }
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

fn history_widget(state: &State, width: u16) -> Paragraph<'static> {
    Paragraph::new(history_lines(state, width))
        .scroll((state.history_scroll as u16, 0))
        .block(pane_block(
            history_title(state, width),
            state,
            Pane::History,
        ))
}

fn history_title(state: &State, width: u16) -> String {
    let mut title = "history".to_string();
    if let Some(visit) = varde::history::selected(state) {
        title.push_str("  ");
        title.push_str(&visit.file);
    }
    truncate(&title, width.saturating_sub(2) as usize)
}

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
            let name = std::path::Path::new(&visit.file)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| visit.file.clone());
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

fn risk_lines(state: &State, width: u16) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(2) as usize;
    varde::risk::list(state)
        .iter()
        .enumerate()
        .map(|(index, function)| {
            let figure = match varde::risk::row_delta(state, function) {
                Some(delta) => format!("{} {delta:+}", function.metrics.cyclomatic),
                None => function.metrics.cyclomatic.to_string(),
            };
            let on_this_row = index == state.risk_selection;
            let selected = if on_this_row {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let actions = if on_this_row {
                varde::risk::row_actions(state)
            } else {
                Vec::new()
            };
            let at_line = format!(":{} ", function.line);
            let room = inner.saturating_sub(
                at_line.chars().count() + figure.chars().count() + 1 + actions.len() * 2,
            );
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
            let icon_style = if selected || row.dimmed {
                style
            } else {
                style.fg(icon_colour(kind))
            };
            let name = format!(" {name}");

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

fn filter_box(frame: &mut Frame, state: &State, area: Rect, draft: Option<&str>) {
    if area.height < 4 {
        return;
    }
    let active = draft.is_some();
    let draft = draft.unwrap_or(&state.filter);
    let inner = area.width.saturating_sub(2);
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

fn review_lines(state: &State, rows: &[tree::Row], width: u16) -> Vec<Line<'static>> {
    let counts = match state.view {
        View::Review => review::diagnostics(state),
        _ => Vec::new(),
    };
    rows.iter()
        .map(|row| {
            let name = varde::relative(state, &row.path);
            let style = match state.tree_selection.as_deref() == Some(row.path.as_path()) {
                true => Style::default().add_modifier(Modifier::REVERSED),
                false => Style::default(),
            };
            let figure = counts
                .iter()
                .find(|(file, _)| state.root.join(file) == row.path)
                .map(|(_, counts)| match counts {
                    None => "—".to_string(),
                    Some(counts) => format!("{}E {}W", counts.errors, counts.warnings),
                });
            let Some(figure) = figure else {
                return Line::from(Span::styled(name, style));
            };
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

fn story_title(state: &State) -> String {
    let Some(left) = &state.left_branch else {
        return "story".to_string();
    };
    let on = state.branch.as_deref().unwrap_or("a detached HEAD");
    format!("story  on {on} (was {left})")
}

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
        story::Set::Downloading { url, how } => {
            let doing = match how {
                story::Download::Clone => "Cloning",
                story::Download::Fetch => "Fetching",
            };
            format!("{doing} {url} in the terminal below…")
        }
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
        story::Set::Refused { because } => format!("The story set could not be read:\n{because}"),
        story::Set::Loaded(_) => "The story set holds no stories.".to_string(),
        story::Set::Authoring { spelling } => format!("Authoring a story for \"{spelling}\"…"),
        story::Set::Filling { .. } => "Reading what the story's sites hold…".to_string(),
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

fn band_widget(state: &State) -> Paragraph<'static> {
    let step = story::current_step(state);
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
    let mut footer = vec![Span::styled(
        format!(" {} ", buffer.pending_command()),
        Style::default().fg(Color::Cyan),
    )];
    footer.extend(dot_spans(state));
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
    let row = |number: usize| shown.binary_search(&number).ok();
    let dark = state.editor_theme != "light";
    let mut lines = code_lines(tokens, shown.iter().copied(), dark, cursor_line(state));
    let bracket = Style::default().bg(if dark {
        Color::Indexed(236)
    } else {
        Color::Indexed(254)
    });
    let guides = buffer.guides(shown.iter().copied());
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
    if let Some((from, to)) = buffer.selected_lines() {
        for (line, _) in lines
            .iter_mut()
            .zip(&shown)
            .filter(|(_, number)| (from..=to).contains(*number))
        {
            line.style = line.style.add_modifier(Modifier::REVERSED);
        }
    }
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
    if state.focus != Pane::Evaluator {
        paint_drag(
            &mut lines,
            &row,
            state.selection.as_ref().and_then(Selection::buffer_span),
            &state.occurrences,
            true,
        );
    }
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
    for (number, values) in varde::debug::inline(state, tokens, varde::fits(state).2) {
        let Some(line) = row(number).map(|index| &mut lines[index]) else {
            continue;
        };
        for value in values {
            let style = match value.changed {
                true => Style::default().add_modifier(Modifier::BOLD),
                false => faint,
            };
            line.spans.push(Span::styled(value.text, style));
        }
    }
    shift(&mut lines, state, 1);
    for (line, &number) in lines.iter_mut().zip(&shown) {
        let Some(drawn) = varde::conflict::drawn(state, number) else {
            continue;
        };
        *line = match drawn {
            varde::conflict::Drawn::Bar(pieces) => {
                let text: String = pieces.into_iter().map(|(piece, _)| piece).collect();
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

fn minimap(frame: &mut Frame, state: &State, areas: &Areas, tokens: &[Vec<highlight::Token>]) {
    let fits = varde::fits(state).1;
    let dark = state.editor_theme != "light";
    let area = areas.minimap;
    if let Some((first, _)) = minimap::mirrored(state) {
        let field = match state.editor_field && dark {
            true => Color::Black,
            false => Color::Reset,
        };
        let slider = match minimap::lit(state) {
            true => Style::new().fg(Color::Reset),
            false => FAINT,
        };
        frame
            .buffer_mut()
            .set_style(area, Style::default().bg(field));
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
                        let (dot, kind) = match (cell.top, cell.bottom) {
                            (Some(kind), Some(_)) => ("\u{2022}", kind),
                            (Some(kind), None) | (None, Some(kind)) => ("\u{00b7}", kind),
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
    if area.width > 0 || !minimap::mirroring(state) {
        return;
    }
    let Some((top, height)) = minimap::thumb(state.editor_scroll, minimap::lines(state), fits)
    else {
        return;
    };
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

fn with_breakpoint(line: Line<'static>, mark: varde::debug::Mark) -> Line<'static> {
    let glyph = match mark {
        varde::debug::Mark::Plain => Span::styled("●", Style::default().fg(Color::Red)),
        varde::debug::Mark::Conditional => Span::styled("◉", Style::default().fg(Color::Red)),
        varde::debug::Mark::Logpoint => Span::styled("◆", Style::default().fg(Color::Yellow)),
        varde::debug::Mark::Stale => Span::styled("◌", Style::default().fg(Color::DarkGray)),
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

pub const DOTS: &str = "⋯";

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

fn dots_style(dark: bool) -> Style {
    match dark {
        true => Style::default().fg(Color::Gray).bg(Color::Indexed(238)),
        false => Style::default().fg(Color::Black).bg(Color::Indexed(252)),
    }
}

fn severity_colour(severity: lsp::Severity) -> Color {
    match severity {
        lsp::Severity::Error => Color::LightRed,
        lsp::Severity::Warning => WARNING,
        lsp::Severity::Information => Color::Blue,
        lsp::Severity::Hint => Color::DarkGray,
    }
}

fn reading_tint(dark: bool) -> Color {
    if dark {
        Color::Rgb(0x22, 0x2a, 0x38)
    } else {
        Color::Indexed(254)
    }
}

fn echo_tint(dark: bool) -> Color {
    if dark {
        Color::Rgb(0x2c, 0x3c, 0x4a)
    } else {
        Color::Indexed(252)
    }
}

fn word_tint(dark: bool) -> Color {
    if dark {
        Color::Rgb(0x33, 0x38, 0x40)
    } else {
        Color::Indexed(253)
    }
}

fn incoming_tint(dark: bool) -> Color {
    if dark {
        Color::Rgb(0x1b, 0x2a, 0x40)
    } else {
        Color::Indexed(189)
    }
}

fn washed(line: &Line<'static>, colour: Color) -> Line<'static> {
    Line::from(
        line.spans
            .iter()
            .map(|span| Span::styled(span.content.clone(), span.style.bg(colour)))
            .collect::<Vec<Span<'static>>>(),
    )
}

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
    if let Some((from, to)) = reading::mark(state) {
        for (line, row) in lines.iter_mut().zip(rows) {
            if row.line >= from && row.line <= to {
                *line = washed(line, reading_tint(dark));
            }
        }
    }
    shift(&mut lines, state, 0);
    Paragraph::new(lines)
        .scroll((state.editor_scroll as u16, 0))
        .block(editor_block(state, title, footer, command, width))
}

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

fn row_style(kind: varde::preview::RowKind, dark: bool) -> Style {
    use varde::preview::RowKind;
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
    if let Some(kind) = piece.token {
        style = style.fg(colour(kind, dark));
    }
    style
}

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

fn buffer_title(
    name: &str,
    buffer: &varde::editor::Buffer,
    mode: &str,
    room: usize,
) -> Line<'static> {
    // ratatui draws a left-aligned title over a right-aligned one, so the name gives first
    let mut trailing = vec![];
    if buffer.is_dirty() {
        trailing.push(Span::styled(" ●", Style::default().fg(DIRTY)));
    }
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
        varde::Tone::Marked => (Color::Yellow, Color::DarkGray, Modifier::BOLD),
    };
    if state.hovered_action == Some(chip.action) {
        lift |= Modifier::BOLD | Modifier::UNDERLINED;
    }
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
    let left = room.saturating_sub(2 + when.width());
    match left {
        0 => String::new(),
        left => format!(" {}{when} ", truncate(who, left)),
    }
}

fn title_room(state: &State, width: u16) -> usize {
    let labels = layout::chip_labels(
        &varde::reading::transport(state),
        width,
        layout::EDITOR_TITLE,
    );
    let strip = match labels.is_empty() {
        true => 0,
        false => layout::strip_width(&labels) as usize + 1,
    };
    (width as usize).saturating_sub(2 + strip)
}

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

fn shift(lines: &mut [Line<'static>], state: &State, chrome: usize) {
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
        // Keep the line's own style: a visual selection lives there, not on the spans
        *line = Line::from(spans).style(line.style);
    }
}

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

fn tokens_of<'a>(code: &'a Code, text: &str) -> Option<&'a Vec<Vec<highlight::Token>>> {
    code.get(text).filter(|lines| {
        lines
            .iter()
            .flatten()
            .any(|token| token.kind != Kind::Plain)
    })
}

fn one_row(lines: &[Vec<highlight::Token>]) -> Vec<highlight::Token> {
    lines.join(&highlight::Token {
        text: "\n".to_string(),
        kind: Kind::Plain,
    })
}

fn cut_spans(
    tokens: &[highlight::Token],
    width: usize,
    style: Style,
    dark: bool,
) -> Vec<Span<'static>> {
    let whole: String = tokens.iter().map(|token| token.text.as_str()).collect();
    let shown = truncate(&whole, width);
    let mut left = match shown == whole {
        true => whole.chars().count(),
        false => shown.chars().count() - 1,
    };
    let mut cut = Vec::new();
    for token in tokens {
        let text: String = token.text.chars().take(left).collect();
        left -= text.chars().count();
        if !text.is_empty() {
            cut.push(Span::styled(text, style.fg(colour(token.kind, dark))));
        }
    }
    if shown != whole {
        cut.push(Span::styled("\u{2026}", style));
    }
    cut.push(Span::styled(
        " ".repeat(width.saturating_sub(shown.width())),
        style,
    ));
    cut
}

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
    story::rows(state, code.len())
        .map(|row| match row {
            story::Row::Code(number) => std::mem::take(&mut code[number as usize - 1]),
            story::Row::Comment(comment) => comment_row(&comment.kind, &comment.body),
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
        refusal(
            story::current_step(state)
                .map(|step| step.site.file.as_str())
                .unwrap_or(relative.as_str()),
        )
    } else {
        marked_code(state, tokens, &marked, &relative)
    };
    let mut footer = dot_spans(state);
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

fn barred(line: &Line<'static>, number: usize, colour: Color) -> Line<'static> {
    let mut spans = vec![
        Span::styled(format!(" {number:>4}"), Style::default().fg(Color::Gray)),
        Span::styled("▌", Style::default().fg(colour)),
        Span::raw(PAD),
    ];
    spans.extend(line.spans.iter().skip(1).cloned());
    Line::from(spans)
}

const PAD: &str = "   ";

fn dimmed(line: &Line<'static>) -> Line<'static> {
    Line::from(
        line.spans
            .iter()
            .map(|span| Span::styled(span.content.clone(), span.style.fg(Color::DarkGray)))
            .collect::<Vec<_>>(),
    )
}

fn bar(kind: story::Kind) -> Color {
    match kind {
        story::Kind::Changed => Color::Yellow,
        story::Kind::Context => Color::Blue,
    }
}

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

fn cheatsheet_widget(state: &State) -> Paragraph<'static> {
    let rows = keys::cheatsheet_rows(state);
    let column = rows.iter().map(|(keys, _)| keys.len()).max().unwrap_or(0);
    Paragraph::new(
        rows.into_iter()
            .skip(state.cheatsheet_scroll)
            .map(|(keys, what)| {
                Line::from(Span::styled(
                    format!(" {keys:column$}  {what}"),
                    Style::default().fg(Color::DarkGray),
                ))
            })
            .collect::<Vec<_>>(),
    )
    .block(pane_block("keys", state, Pane::Cheatsheet))
}

fn hover(frame: &mut Frame, state: &State, panes: &layout::Layout) {
    let (Some(hover), Some(placement)) = (state.hover.as_ref(), lsp::placement(state)) else {
        return;
    };
    let dark = state.editor_theme != "light";
    let columns = placement.width.saturating_sub(2);
    let lines = lsp::sections(state)
        .iter()
        .skip(hover.first)
        .map(|said| match said {
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

fn evaluator(frame: &mut Frame, state: &State, panes: &layout::Layout, code: &Code) {
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
    let dark = state.editor_theme != "light";
    let mut lines = snippet_lines(open.snippet.shown(), code, dark);
    if state.focus == Pane::Evaluator {
        paint_drag(
            &mut lines,
            &|number| Some(number - 1),
            state.selection.as_ref().and_then(Selection::buffer_span),
            &state.occurrences,
            false,
        );
    }
    frame.render_widget(Paragraph::new(lines), rect(snippet_area));
    frame.render_widget(
        Block::default().borders(Borders::TOP),
        rect(varde::layout::Area {
            height: 1,
            y: output_area.y.saturating_sub(1),
            ..output_area
        }),
    );
    let output = output_lines(&varde::debug::evaluator_output(state), code, dark);
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

fn snippet_lines(text: &str, code: &Code, dark: bool) -> Vec<Line<'static>> {
    match tokens_of(code, text) {
        Some(lines) => lines
            .iter()
            .map(|line| Line::from(spans(line, dark)))
            .collect(),
        None => text
            .split('\n')
            .map(|row| Line::raw(row.to_string()))
            .collect(),
    }
}

fn output_lines(said: &[varde::debug::Said], code: &Code, dark: bool) -> Vec<Line<'static>> {
    said.iter()
        .map(|line| match line {
            varde::debug::Said::Printed(text) => Line::from(Span::styled(
                text.clone(),
                Style::default().fg(Color::DarkGray),
            )),
            varde::debug::Said::Running => Line::from(Span::styled(
                "running\u{2026}",
                Style::default().fg(Color::Yellow),
            )),
            varde::debug::Said::Failed(why) => Line::from(Span::styled(
                why.clone(),
                Style::default().fg(Color::LightRed),
            )),
            varde::debug::Said::Value(row) => {
                let mut line = Line::from(Span::styled(
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
                ));
                match tokens_of(code, &row.value) {
                    Some(lines) => {
                        for span in spans(&one_row(lines), dark) {
                            line.push_span(span);
                        }
                    }
                    None => line.push_span(Span::raw(row.value.clone())),
                }
                line
            }
        })
        .collect()
}

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

fn at_caret(query: &varde::editor::Buffer) -> (String, String) {
    let text = query.shown();
    let at = text
        .char_indices()
        .nth(query.column - 1)
        .map_or(text.len(), |(at, _)| at);
    (text[..at].to_string(), text[at..].to_string())
}

fn editor_block(
    state: &State,
    title: Line<'static>,
    footer: Vec<Span<'static>>,
    command: Option<&str>,
    width: u16,
) -> Block<'static> {
    let room = title_room(state, width).saturating_sub(title.width());
    pane_block(title, state, Pane::Editor)
        .title(right_title(state, room, width))
        .title_bottom(Line::from(footer).right_aligned())
        .title_bottom(command_line(state, command).left_aligned())
}

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
            Some(tokens) => spans.extend(
                self::spans(tokens, dark)
                    .into_iter()
                    .map(|span| Span::styled(span.content, span.style.patch(row))),
            ),
            None => spans.push(Span::styled(line.text.clone(), row.fg(own))),
        }
        rows.push(Line::from(spans));
    }
    shift(&mut rows, state, 2);
    let mut lines: Vec<Line> = Vec::new();
    for (index, line) in diff.iter().enumerate() {
        lines.push(std::mem::take(&mut rows[index]));
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

fn change_colours(removed: bool, dark: bool) -> (char, Color, Color) {
    match (removed, dark) {
        (true, true) => ('-', Color::Indexed(167), Color::Rgb(0x3a, 0x20, 0x24)),
        (true, false) => ('-', Color::Indexed(167), Color::Indexed(224)),
        (false, true) => ('+', Color::Indexed(71), Color::Rgb(0x1b, 0x35, 0x24)),
        (false, false) => ('+', Color::Indexed(71), Color::Indexed(194)),
    }
}

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

fn comment_row(kind: &str, body: &str) -> Line<'static> {
    // A ratatui Line does not break on \n, and a comment must stay exactly one row
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

fn caret_at(state: &State) -> Option<(usize, usize)> {
    match (&state.diff, state.current_buffer.as_ref()) {
        (Some(_), _) => Some((state.diff_line, 1)),
        (None, Some(path)) if varde::previewing(state) => state
            .buffers
            .get(path)
            .map(|buffer| (buffer.row, buffer.row_column)),
        (None, Some(path)) => state
            .buffers
            .get(path)
            .map(|buffer| (story::row_of(state, buffer.line as u32), buffer.column)),
        _ => None,
    }
}

fn place_cursor(frame: &mut Frame, state: &State, areas: &Areas, command: Option<&str>) {
    if let Some(line) = command {
        frame.set_cursor_position((
            areas.editor.x + 2 + line.chars().count() as u16,
            areas.editor.bottom().saturating_sub(1),
        ));
        return;
    }
    let claimed = !matches!(state.modal, Modal::None | Modal::Candidates(_));
    if state.focus != Pane::Editor || claimed {
        return;
    }
    if story::refused(state) {
        return;
    }
    let Some((line, column)) = caret_at(state) else {
        return;
    };
    let Some(row) = line.saturating_sub(1).checked_sub(state.editor_scroll) else {
        return;
    };
    let Some(column) = column.checked_sub(1 + state.editor_hscroll) else {
        return;
    };
    let x = areas.editor.x + 1 + varde::gutter(state) + column as u16;
    let y = areas.editor.y + 1 + row as u16;
    if x < areas.editor.right().saturating_sub(1) && y < areas.editor.bottom().saturating_sub(1) {
        frame.set_cursor_position((x, y));
    }
}

/// ratatui hides the cursor on any frame that does not set it
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

fn glyph(mark: Mark) -> (&'static str, Color) {
    match mark {
        Mark::None => (" ", Color::Reset),
        Mark::Open => ("○", Color::DarkGray),
        Mark::Dirty => ("●", DIRTY),
        Mark::Current => ("●", Color::Cyan),
        Mark::CurrentDirty => ("●", DIRTY),
    }
}

fn guided(
    line: &Line<'static>,
    guides: &[varde::editor::Guide],
    brackets: &[usize],
    mark: Style,
    faint: Style,
) -> Line<'static> {
    // A heavy box glyph rather than BOLD, which terminals may ignore on box-drawing characters
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
                    // Tabs are never dotted: one character spanning many columns would shift the rest of the line
                    match character {
                        ' ' => spans.push(Span::styled("\u{00b7}", style.patch(faint))),
                        _ => spans.push(Span::styled(character.to_string(), style)),
                    }
                }
            }
            column += 1;
        }
    }
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
        (Kind::Plain, true) => Color::Rgb(0x9c, 0xdc, 0xfe),
        (Kind::Plain, false) => Color::Rgb(0x00, 0x10, 0x80),
    }
}

fn terminal_widget(
    pane: &PtyPane,
    title: &str,
    focused: bool,
    state: &State,
    which: Pane,
) -> Paragraph<'static> {
    let screen = pane.screen();
    let (rows, columns) = screen.size();
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
    debug_assert_eq!(lines.len(), layout::SEARCH_HEADER as usize);

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
    // Skipped, not scrolled: the widget's own offset would scroll the header off first
    lines.extend(list.into_iter().skip(search.scroll));
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
            false => format!("  No branch matches {filter:?}."),
        }));
    }
    let mut lines = window_on(lines, selected, height, 5);
    lines.push(Line::from(""));
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

fn window_on(lines: Vec<Line<'static>>, at: usize, height: u16, chrome: u16) -> Vec<Line<'static>> {
    let room = height.saturating_sub(chrome).max(1) as usize;
    let start = (at + 1).saturating_sub(room);
    lines.into_iter().skip(start).take(room).collect()
}

fn rows_lines(rows: Vec<(Option<char>, String)>) -> Vec<Line<'static>> {
    rows.into_iter()
        .map(|(key, row)| match key {
            Some(_) => Line::from(row),
            None => Line::from(Span::styled(row, Style::default().fg(Color::DarkGray))),
        })
        .collect()
}

fn overlay_measure(width: u16) -> usize {
    width.saturating_sub(4).max(20) as usize
}

/// Not Paragraph::wrap: the box is sized from the line count, which render-time wrapping breaks
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

#[cfg(test)]
mod tests {
    use super::{
        action_icon, authorship_clause, branch_lines, buffer_title, code_lines, colour, diff_rows,
        editor_block, faint, guided, highlight, icon_colour, launch_lines, layout, output_lines,
        paint_drag, pane_actions_title, preview_line, right_title, risk_lines, risk_title, shift,
        snippet_lines, source_lines, status_line, story_title, title_room, tree_lines, truncate,
        variables_lines, with_breakpoint, with_caret, Block, Borders, Code, Color, Kind, Line,
        Modifier, Place, Selection, Span, State, Style, Tone, UnicodeWidthStr, DIRTY, DOTS,
        WARNING,
    };
    use varde::risk::{Figure, Figures, Function, Metrics};

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
        assert_eq!(authorship_clause(&state, 12), "");

        state.traced = Some((path, 1, traced("fn main() { run() }\n")));
        assert_eq!(authorship_clause(&state, 40), " Not committed yet ");
    }

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
        assert!(filtered.contains(&"   filter: zzz".to_string()));
        assert!(text(branch_lines(&["main".to_string()], "", 0, 26))
            .contains(&"   type to filter".to_string()));
    }

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
        assert_eq!(lines.len() + 2, 20);
        assert_eq!(lines.first().map(String::as_str), Some("  branch-16"));
        assert_eq!(lines[14], "> branch-30");
    }

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
        assert_eq!(lines.len() + 2, 20);
        assert_eq!(lines.first().map(String::as_str), Some("  launch-25"));
        assert_eq!(lines[15], "> launch-40");
    }
    use varde::tree::{IconKind, Row};
    use varde::{DiffLine, Event};

    #[test]
    fn the_story_border_names_the_branch_the_poll_last_read() {
        assert_eq!(story_title(&State::default()), "story");
        let mut state = State::default();
        state.left_branch = Some("main".to_string());
        state.branch = Some("feature".to_string());
        assert_eq!(story_title(&state), "story  on feature (was main)");
        state.branch = Some("something-else".to_string());
        assert_eq!(story_title(&state), "story  on something-else (was main)");
    }

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

    #[test]
    fn the_risk_pane_spends_every_row_inside_its_borders_on_a_function() {
        let state = measured();
        let lines = risk_lines(&state, 30);
        assert_eq!(lines.len(), 2, "a row that is not a Function: {lines:?}");
        let drawn: Vec<String> = lines.iter().map(Line::to_string).collect();
        assert!(drawn[0].starts_with(" route"), "{drawn:?}");
        assert!(drawn[0].contains("31"), "{drawn:?}");
        assert!(drawn[1].starts_with(" draw"), "{drawn:?}");
        assert!(drawn[1].ends_with("22"), "{drawn:?}");
        assert!(
            drawn.iter().all(|row| !row.contains("src/")),
            "a path in a row: {drawn:?}"
        );
        assert!(
            drawn.iter().all(|row| row.chars().count() == 28),
            "{drawn:?}"
        );
    }

    #[test]
    fn a_risk_row_names_the_line_its_function_starts_on() {
        let state = measured();
        let drawn: Vec<String> = risk_lines(&state, 30).iter().map(Line::to_string).collect();
        assert!(drawn[1].ends_with(":17 22"), "{drawn:?}");
        let narrow: Vec<String> = risk_lines(&state, 12).iter().map(Line::to_string).collect();
        assert!(narrow[1].contains(":17"), "{narrow:?}");
        assert!(!narrow[1].contains("draw"), "{narrow:?}");
        assert_eq!(risk_lines(&state, 9).remove(1).to_string(), ":17 22");
        let spans = risk_lines(&state, 30).remove(1).spans;
        assert_eq!(spans[1].content, ":17 ");
        assert_eq!(spans[1].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[2].style.fg, Some(WARNING));
    }

    #[test]
    fn a_row_with_nothing_measured_still_names_its_line() {
        let mut state = measured();
        state.risk_all = true;
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
        assert_eq!(top[25..29], icons[..], "{top:?}");
        assert_eq!(buffer[(25, 0)].style().fg, Some(Color::DarkGray));
        let mut armed = state;
        armed.focus = varde::Pane::Risk;
        armed.risk_selection = 2;
        armed.selected_action = Some(1);
        assert!(varde::risk::on_actions(&armed));
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        super::pane_block("risk", &armed, varde::Pane::Risk)
            .title(pane_actions_title(&armed))
            .render(area, &mut buffer);
        assert_eq!(buffer[(25, 0)].style().fg, Some(Color::DarkGray));
        assert_eq!(buffer[(27, 0)].style().fg, Some(Color::Cyan), "the loop");

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
                .title(right_title(&state, 0, width))
                .render(area, &mut buffer);
            let top: String = (0..width)
                .map(|column| buffer[(column, 0)].symbol().to_string())
                .collect();
            assert!(top.ends_with(&format!("{drawn}\u{2510}")), "{top:?}");
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
        state.transport_lit = Some(varde::reading::STOP);
        state.hovered_action = None;
        let spans = right_title(&state, 0, 120).spans;
        let stop = spans
            .iter()
            .find(|span| span.content.contains('\u{25a0}'))
            .expect("the stop Chip");
        assert_eq!(stop.style.fg, Some(Color::Red));
        assert!(stop.style.add_modifier.contains(Modifier::REVERSED));

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

        let top = drawn(
            &state,
            "a-very-long-document-name-indeed-and-then-some-more-of-it.md",
        );
        assert!(top.contains("indeed-and-then"), "the name gave: {top:?}");
        assert!(!top.contains("Ada"), "{top:?}");
        assert!(top.ends_with(" 1.00x \u{2500}\u{2510}"), "{top:?}");
    }

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
            .title(right_title(&state, 0, 40))
            .render(area, &mut buffer);
        let top: String = (0..40)
            .map(|column| buffer[(column, 0)].symbol().to_string())
            .collect();
        assert!(top.contains('\u{2026}'), "the name was not cut: {top:?}");
        assert!(top.ends_with(" 1.00x \u{2500}\u{2510}"), "{top:?}");
        assert!(top.contains("[normal]\u{2500} \u{25ba}"), "{top:?}");
    }

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
            let name = path.file_name().expect("a name").to_str().expect("utf-8");
            assert_eq!(
                rows[index].spans[1].content,
                format!(" {name}"),
                "{:?}",
                rows[index]
            );
        }
        assert_ne!(rows[0].spans[0].style.fg, rows[1].spans[0].style.fg);
    }

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
        assert_eq!(before, (30 - 2) - 2);
    }

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
        assert_eq!(before, (30 - 2) - 2, "{line:?}");
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
        state.history_selection = state.visits.len();
        assert_eq!(super::history_title(&state, 40), "history");
        state.history_selection = 0;
        assert_eq!(super::history_title(&state, 40), "history  src/editor.rs");
        state.history_selection = 1;
        assert_eq!(
            super::history_title(&state, 40),
            "history  src/keys/reserved.rs"
        );
        assert_eq!(
            super::history_title(&state, 30),
            "history  src/keys/reserved.…"
        );
    }

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
        assert!(
            stale.spans[1].content.starts_with("pub struct Nothing"),
            "{stale:?}"
        );
    }

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
        assert_eq!(risk_title(&state, 30), "risk  src/keys.rs  stal…");
        state.risk.figure = Figure::None;
        assert_eq!(risk_title(&state, 30), "risk  nothing measured");
        assert!(risk_lines(&state, 30).is_empty(), "a list with no figure");
        let _ = varde::risk::analyse(&mut state, varde::risk::Scope::Workspace);
        assert_eq!(risk_title(&state, 30), "risk  measuring");
    }

    #[test]
    fn the_border_names_the_file_of_the_row_the_marker_is_on() {
        let mut state = measured();
        state.risk_selection = 1;
        assert_eq!(risk_title(&state, 30), "risk  src/ui.rs");
        let lines = risk_lines(&state, 30);
        let marked = |line: &Line<'static>| {
            line.spans[..2]
                .iter()
                .all(|span| span.style.add_modifier.contains(Modifier::REVERSED))
        };
        assert!(!marked(&lines[0]), "an unselected row is marked");
        assert!(marked(&lines[1]), "the selected row is not marked");
        state.risk_selection = 9;
        assert_eq!(risk_title(&state, 30), "risk");
        assert!(risk_lines(&state, 30).iter().all(|line| !marked(line)));
    }

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

    fn drawn(state: &State, diff: &[DiffLine]) -> Vec<Line<'static>> {
        let new = highlight::highlight("a.ts", "const kept = 1;\nconst gone = 2;");
        let old = highlight::highlight("a.ts", "const kept = 1;\nconst gone = \"old\";");
        diff_rows(state, diff, "a.ts", &new, &old)
    }

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

    #[test]
    fn a_diff_row_is_coloured_by_its_own_sides_language() {
        let (state, diff) = diffed();
        let rows = drawn(&state, &diff);
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
        let flat = diff_rows(&state, &diff, "a.ts", &[], &[]);
        for (index, row) in flat.iter().enumerate() {
            assert_eq!(row.spans.len(), 3, "{row:?}");
            assert_eq!(row.spans[2].style.fg, row.spans[1].style.fg, "row {index}");
        }
    }

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
        assert_eq!(rows.len(), diff.len() + 1);
        assert!(rows[3].to_string().contains("ISSUE still unquoted"));
        assert_eq!(rows[3].spans[0].style.fg, Some(Color::Red));
        assert_eq!(
            rows[3].spans[0].style.bg, None,
            "a comment took a row's tint"
        );
    }

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
        assert!(
            drawn.contains("ghijklmnop"),
            "the fence did not slide: {drawn:?}"
        );
    }

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

    #[test]
    fn the_label_fills_the_columns_the_actions_are_hit_tested_from() {
        let (mut state, rows) = one_file_tree();
        state.tree_selection = Some(rows[0].path.clone());
        let width = 40;
        let line = tree_lines(&state, &rows, width).remove(0);
        assert_eq!(
            varde::tree::row_actions(&state, &rows[0].path).len(),
            2,
            "a file offers delete and copy-path, so this is the padded branch"
        );
        let label: usize = line.spans[1..4]
            .iter()
            .map(|span| span.content.chars().count())
            .sum();
        assert_eq!(label, (width as usize - 3) - 4);
    }

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

    #[test]
    fn shifting_into_a_span_keeps_its_style() {
        let mut state = State::default();
        state.editor_hscroll = 2;
        let mut lines = [row()];
        shift(&mut lines, &state, 1);
        assert_eq!(lines[0].spans[1].content, "t ");
        assert_eq!(lines[0].spans[1].style.fg, Some(Color::Magenta));
    }

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

    #[test]
    fn a_wide_glyph_is_not_split_across_the_cut() {
        assert_eq!(truncate("ああああ", 5), "ああ…");
    }

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
        assert_eq!(drawn("abc", 4), "abc[ ]");
        assert_eq!(drawn("", 1), "[ ]");
        assert_eq!(drawn("nåværende", 3), "nå[v]ærende");
        assert_eq!(drawn("ab", 99), "ab[ ]");
    }

    fn text(line: Line) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

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

    fn coloured_rust(texts: &[&str]) -> Code {
        texts
            .iter()
            .map(|text| (text.to_string(), highlight::highlight("one.rs", text)))
            .collect()
    }

    fn colours(line: &Line) -> Vec<Option<Color>> {
        line.spans.iter().map(|span| span.style.fg).collect()
    }

    #[test]
    fn a_variables_value_is_coloured_and_its_name_is_not() {
        let value = "Order { name: \"Ann\", total: 42 }";
        let mut state = State::default();
        state.watches = vec![varde::debug::Watch {
            expression: "order".to_string(),
            answer: varde::debug::Answer::Value(value.to_string()),
        }];
        let line = &variables_lines(&state, 80, "", &coloured_rust(&[value]))[0];
        assert!(line.spans[0].content.contains("order"));
        assert_eq!(line.spans[0].style.fg, None);
        let shown = colours(line);
        assert!(
            shown.contains(&Some(colour(Kind::String, true))),
            "{shown:?}"
        );
        assert!(
            shown.contains(&Some(colour(Kind::Number, true))),
            "{shown:?}"
        );

        let unknown: Code = [(value.to_string(), highlight::plain(value))].into();
        for code in [Code::new(), unknown] {
            let plain = &variables_lines(&state, 80, "", &code)[0];
            assert_eq!(plain.width(), line.width());
            assert_eq!(plain.spans[2].style.fg, Some(Color::DarkGray));
        }
    }

    #[test]
    fn a_coloured_value_is_cut_to_its_room() {
        let tokens = highlight::highlight("one.rs", "\"Ann\", 42");
        let cut = super::cut_spans(&tokens.concat(), 5, Style::default(), true);
        let text: String = cut.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(text, "\"Ann\u{2026}");
    }

    #[test]
    fn a_coloured_value_keeps_its_newlines() {
        let value = "[\n    1,\n]";
        let row = super::one_row(&highlight::highlight("one.rs", value));
        let text: String = row.iter().map(|token| token.text.as_str()).collect();
        assert_eq!(text, value);
    }

    #[test]
    fn the_snippet_colours_a_string_across_its_lines() {
        let snippet = "let s = \"one\ntwo\";";
        let lines = snippet_lines(snippet, &coloured_rust(&[snippet]), true);
        assert_eq!(lines[1].spans[0].content, "two\"");
        assert_eq!(lines[1].spans[0].style.fg, Some(colour(Kind::String, true)));
        let plain = snippet_lines(snippet, &Code::new(), true);
        assert!(plain
            .iter()
            .flat_map(|line| colours(line))
            .all(|fg| fg.is_none()));
    }

    #[test]
    fn the_evaluator_colours_the_value_and_not_the_prints() {
        let row = varde::debug::Row {
            name: "total".to_string(),
            value: "\"Ann\"".to_string(),
            depth: 0,
            hint: varde::debug::Hint::Plain,
            open: false,
            opens: varde::debug::Opens::Nothing,
            expression: String::new(),
            parent: 0,
            of: varde::debug::Of::Member,
        };
        let said = [
            varde::debug::Said::Printed("\"printed\" 7".to_string()),
            varde::debug::Said::Value(row),
        ];
        let lines = output_lines(&said, &coloured_rust(&["\"printed\" 7", "\"Ann\""]), true);
        assert_eq!(colours(&lines[0]), vec![Some(Color::DarkGray)]);
        assert!(colours(&lines[1]).contains(&Some(colour(Kind::String, true))));
    }
}
