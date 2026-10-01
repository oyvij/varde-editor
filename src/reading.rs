use crate::{preview, Effect, Selection, State};
use std::path::{Path, PathBuf};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub utterances: Vec<Utterance>,
    pub offsets: Vec<u32>,
    pub at_ms: u32,
    pub paused: bool,
    pub file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seek {
    To(u32),
    Ended,
    Nowhere,
}

impl Reading {
    pub fn at(&self) -> usize {
        self.offsets
            .partition_point(|start| *start <= self.at_ms)
            .saturating_sub(1)
    }

    pub fn forward(&self) -> Seek {
        if self.offsets.is_empty() {
            return Seek::Nowhere;
        }
        match self.offsets.get(self.at() + 1) {
            Some(start) => Seek::To(*start),
            None => Seek::Ended,
        }
    }

    pub fn back(&self) -> Seek {
        match self.offsets.get(self.at().saturating_sub(1)) {
            Some(start) => Seek::To(*start),
            None => Seek::Nowhere,
        }
    }
}

pub fn mark(state: &State) -> Option<(usize, usize)> {
    let reading = state.reading.as_ref()?;
    if reading.file.as_deref()? != state.current_buffer.as_deref()? {
        return None;
    }
    Some(reading.utterances.get(reading.at())?.lines)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utterance {
    pub text: String,
    pub gap_ms: u32,
    pub lines: (usize, usize),
}

pub fn words(utterances: &[Utterance]) -> String {
    utterances
        .iter()
        .map(|one| one.text.as_str())
        .collect::<Vec<&str>>()
        .join(" ")
}

pub const SENTENCE_GAP_MS: u32 = 550;
pub const PARAGRAPH_GAP_MS: u32 = 1000;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Speech {
    pub command: String,
    pub args: Vec<String>,
    pub voice: String,
    pub speed: f32,
    pub player: String,
    pub install: String,
}

pub fn voice_file(voice: &str, home: &Path) -> Option<PathBuf> {
    if voice.is_empty() {
        return None;
    }
    Some(match Path::new(voice).strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => PathBuf::from(voice),
    })
}

pub fn duration_scale(speed: f32) -> f32 {
    if speed > 0.0 {
        1.0 / speed
    } else {
        1.0
    }
}

pub type Refused = (&'static str, Option<&'static str>);

pub fn start(state: &State) -> Result<(Reading, Vec<Effect>), Refused> {
    let Some(path) = state.current_buffer.as_ref() else {
        return Err(("nothing-selected", None));
    };
    if !preview::is_markdown(path) {
        return Err(("not-markdown", None));
    }
    let Some(source) = state.selected_text() else {
        return Err(("nothing-selected", None));
    };
    if let Some(missing) = missing(state) {
        return Err(missing);
    }
    let anchor = match state.selection.as_ref() {
        Some(Selection::Lines { from, .. }) => Some(*from),
        Some(charwise @ Selection::Buffer { .. }) => {
            charwise.buffer_span().map(|(from, _)| from.line)
        }
        Some(Selection::Screen { .. }) | None => None,
    };
    let mut utterances = utterances(&source);
    if let Some(first) = anchor {
        for one in &mut utterances {
            one.lines = (one.lines.0 + first - 1, one.lines.1 + first - 1);
        }
    }
    Ok((
        Reading {
            utterances: utterances.clone(),
            offsets: Vec::new(),
            at_ms: 0,
            paused: false,
            file: anchor.is_some().then(|| path.clone()),
        },
        vec![Effect::Speak {
            utterances,
            speed: state.speech.speed,
        }],
    ))
}

fn missing(state: &State) -> Option<Refused> {
    if !state.voice_installed {
        return Some(("no-voice", Some("synthesizer")));
    }
    if !state.voice_running {
        return Some(("no-synthesizer", Some("synthesizer")));
    }
    if !state.player_installed {
        return Some(("no-player", Some("player")));
    }
    None
}

pub fn utterances(source: &str) -> Vec<Utterance> {
    let mut spoken = Vec::new();
    for block in blocks(source) {
        spoken.extend(block.text.unicode_sentences().map(|sentence| Utterance {
            text: sentence.trim().to_string(),
            gap_ms: SENTENCE_GAP_MS,
            lines: (block.from, block.to),
        }));
        if let Some(last) = spoken.last_mut() {
            last.gap_ms = PARAGRAPH_GAP_MS;
        }
    }
    if let Some(last) = spoken.last_mut() {
        last.gap_ms = 0;
    }
    spoken
}

fn blocks(source: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut fresh = true;
    for row in preview::rows(source, 0) {
        if row.pieces.is_empty() {
            fresh = true;
            continue;
        }
        if fresh || starts_an_item(&row) {
            blocks.push(Block {
                text: String::new(),
                from: row.line,
                to: row.line,
            });
            fresh = false;
        }
        let last = blocks.last_mut().expect("a block to put the row in");
        if !last.text.is_empty() {
            last.text.push(' ');
        }
        last.text.push_str(&row.text());
    }
    blocks.retain(|block| !block.text.trim().is_empty());
    let lines: Vec<&str> = source.lines().collect();
    for index in 0..blocks.len() {
        let next = blocks
            .get(index + 1)
            .map_or(lines.len(), |block| block.from - 1);
        let mut to = next.max(blocks[index].from);
        while to > blocks[index].from && lines.get(to - 1).is_none_or(|line| line.trim().is_empty())
        {
            to -= 1;
        }
        blocks[index].to = to;
    }
    blocks
}

struct Block {
    text: String,
    from: usize,
    to: usize,
}

fn starts_an_item(row: &preview::Row) -> bool {
    matches!(
        row.kind,
        preview::RowKind::List(preview::ListItem {
            marker: Some(_),
            ..
        })
    )
}

pub const PREVIOUS: &str = "reading-previous";
pub const PLAY_PAUSE: &str = "reading-play-pause";
pub const NEXT: &str = "reading-next";
pub const STOP: &str = "reading-stop";
pub const SPEED: &str = "reading-speed";

pub const SPEEDS: [f32; 5] = [0.75, 1.0, 1.25, 1.5, 2.0];

pub fn next_speed(current: f32) -> f32 {
    SPEEDS
        .iter()
        .copied()
        .find(|speed| *speed > current + 0.01)
        .unwrap_or(SPEEDS[0])
}

/// U+23EE and U+25B6/U+25FC render emoji-wide in most fonts despite unicode-width; keep these glyphs
pub fn transport(state: &State) -> Vec<crate::Chip> {
    use crate::{Chip, Hue, Tone};
    if state.diff.is_some() || state.walking.is_some() {
        return Vec::new();
    }
    let readable = state
        .current_buffer
        .as_ref()
        .is_some_and(|path| preview::is_markdown(path));
    if !readable {
        return Vec::new();
    }
    let playing = matches!(&state.reading, Some(reading) if !reading.paused);
    let chip = |action, name, glyph: &str, keys, hue, needs_reading: bool| Chip {
        action,
        name,
        glyph: glyph.to_string(),
        keys,
        hue,
        tone: match state.transport_lit == Some(action) {
            true => Tone::Lit,
            false if needs_reading && state.reading.is_none() => Tone::Dimmed,
            false => Tone::Plain,
        },
    };
    vec![
        match playing {
            true => chip(PLAY_PAUSE, "pause", "\u{25ae}", ":pause", Hue::Hold, false),
            false => chip(PLAY_PAUSE, "play", "\u{25ba}", ":pause", Hue::Go, false),
        },
        chip(PREVIOUS, "previous", "\u{ab}", ":prev", Hue::Step, true),
        chip(NEXT, "next", "\u{bb}", ":next", Hue::Step, true),
        chip(STOP, "stop", "\u{25a0}", ":stop", Hue::Halt, true),
        Chip {
            glyph: format!("{:.2}x", state.speech.speed),
            ..chip(SPEED, "speed", "", ":speed", Hue::Plain, false)
        },
    ]
}

/// Some synthesizers log `INFO:...:Wrote /path.wav`, so take the last token, not the whole line
pub fn wrote(line: &str) -> Option<&str> {
    line.split_whitespace()
        .next_back()
        .filter(|token| token.ends_with(".wav"))
}

#[cfg(test)]
mod tests {
    use super::{
        duration_scale, mark, next_speed, transport, utterances, voice_file, words, wrote, Reading,
        Seek, Utterance, NEXT, PARAGRAPH_GAP_MS, PLAY_PAUSE, PREVIOUS, SENTENCE_GAP_MS, SPEED,
        STOP,
    };
    use crate::State;
    use std::path::{Path, PathBuf};

    fn spoken(source: &str) -> String {
        words(&utterances(source))
    }

    #[test]
    fn a_heading_is_read_as_the_words_it_contains() {
        assert_eq!(spoken("# Setup"), "Setup");
        assert_eq!(spoken("### Deeper still"), "Deeper still");
    }

    #[test]
    fn emphasis_and_code_spans_lose_their_punctuation() {
        assert_eq!(spoken("Install **now**."), "Install now.");
        assert_eq!(spoken("Run `cargo test` first."), "Run cargo test first.");
    }

    #[test]
    fn a_link_is_read_as_its_text_and_never_its_url() {
        assert_eq!(spoken("See [the guide](http://x.test)."), "See the guide.");
    }

    #[test]
    fn markers_are_not_spoken() {
        assert_eq!(spoken("- one"), "one");
        assert_eq!(spoken("> Careful."), "Careful.");
    }

    #[test]
    fn a_code_block_is_read_like_any_other_block() {
        assert_eq!(
            spoken("Run this:\n\n```sh\ncargo test\n```"),
            "Run this: cargo test"
        );
    }

    #[test]
    fn the_blank_rows_between_blocks_are_not_spoken() {
        assert_eq!(spoken("One.\n\nTwo.\n\nThree."), "One. Two. Three.");
    }

    fn said(source: &str) -> Vec<String> {
        utterances(source)
            .iter()
            .map(|one| one.text.clone())
            .collect()
    }

    fn gaps(source: &str) -> Vec<u32> {
        utterances(source).iter().map(|one| one.gap_ms).collect()
    }

    #[test]
    fn one_utterance_per_sentence_in_order() {
        assert_eq!(
            said("Install it. Then run it. Read the output."),
            ["Install it.", "Then run it.", "Read the output."]
        );
    }

    #[test]
    fn a_question_and_an_exclamation_end_one_too() {
        assert_eq!(
            said("Is it on? Try it! Again."),
            ["Is it on?", "Try it!", "Again."]
        );
    }

    #[test]
    fn a_colon_does_not_end_an_utterance() {
        assert_eq!(
            said("An IDE TUI: a terminal UI that opens on a folder."),
            ["An IDE TUI: a terminal UI that opens on a folder."]
        );
    }

    #[test]
    fn a_dot_inside_a_number_or_an_abbreviation_does_not_end_one() {
        assert_eq!(
            said("It goes to 1.0 rather than 0.8."),
            ["It goes to 1.0 rather than 0.8."]
        );
        assert_eq!(
            said("R3.3 says paths are absolute."),
            ["R3.3 says paths are absolute."]
        );
    }

    #[test]
    fn a_sentence_wrapped_across_rows_is_one_utterance() {
        assert_eq!(
            said("Install it\nand then run it."),
            ["Install it and then run it."]
        );
    }

    #[test]
    fn silence_between_sentences_and_none_after_the_last() {
        assert_eq!(
            gaps("Install it. Then run it. Read the output."),
            [SENTENCE_GAP_MS, SENTENCE_GAP_MS, 0]
        );
    }

    #[test]
    fn a_paragraph_ends_with_a_longer_silence_than_a_sentence() {
        assert_eq!(
            gaps("One. Two.\n\nThree. Four."),
            [SENTENCE_GAP_MS, PARAGRAPH_GAP_MS, SENTENCE_GAP_MS, 0]
        );
    }

    #[test]
    fn a_heading_is_a_block_of_its_own() {
        assert_eq!(
            utterances("# Setup\n\nInstall it."),
            [
                Utterance {
                    text: "Setup".to_string(),
                    gap_ms: PARAGRAPH_GAP_MS,
                    lines: (1, 1)
                },
                Utterance {
                    text: "Install it.".to_string(),
                    gap_ms: 0,
                    lines: (3, 3)
                },
            ]
        );
    }

    #[test]
    fn each_list_item_is_its_own_utterance_with_a_pause_after_it() {
        assert_eq!(said("- one\n- two\n- three"), ["one", "two", "three"]);
        assert_eq!(
            gaps("- one\n- two\n- three"),
            [PARAGRAPH_GAP_MS, PARAGRAPH_GAP_MS, 0]
        );
    }

    #[test]
    fn a_list_item_holding_two_sentences_is_two_utterances_and_one_item() {
        assert_eq!(
            said("- Install it. Then run it.\n- Read the output."),
            ["Install it.", "Then run it.", "Read the output."]
        );
        assert_eq!(
            gaps("- Install it. Then run it.\n- Read the output."),
            [SENTENCE_GAP_MS, PARAGRAPH_GAP_MS, 0]
        );
    }

    fn marked(source: &str) -> Vec<(usize, usize)> {
        utterances(source).iter().map(|one| one.lines).collect()
    }

    #[test]
    fn nothing_is_marked_over_a_buffer_the_passage_did_not_come_from() {
        let guide = std::path::PathBuf::from("/w/guide.md");
        let mut state = State {
            current_buffer: Some(guide.clone()),
            reading: Some(Reading {
                file: Some(guide),
                ..playing(0)
            }),
            ..State::default()
        };
        assert_eq!(mark(&state), Some((1, 1)));
        state.current_buffer = Some(std::path::PathBuf::from("/w/other.md"));
        assert_eq!(mark(&state), None);
    }

    #[test]
    fn an_utterance_carries_the_lines_it_was_written_on() {
        assert_eq!(marked("One.\n\nTwo.\n\nThree."), [(1, 1), (3, 3), (5, 5)]);
        assert_eq!(marked("One. Two."), [(1, 1), (1, 1)]);
    }

    #[test]
    fn a_paragraph_is_marked_on_every_line_it_was_written_across() {
        assert_eq!(marked("Install it\nand then run it."), [(1, 2)]);
        assert_eq!(marked("Install it\nand run it. Read it."), [(1, 2), (1, 2)]);
    }

    #[test]
    fn the_blank_line_after_a_block_is_not_marked() {
        assert_eq!(
            marked("- one\n- two\n  wrapped\n\nAfter."),
            [(1, 1), (2, 3), (5, 5)]
        );
    }

    fn playing(at_ms: u32) -> Reading {
        Reading {
            utterances: utterances("One. Two. Three."),
            offsets: vec![0, 1_550, 3_100],
            at_ms,
            paused: false,
            file: None,
        }
    }

    #[test]
    fn the_current_utterance_is_the_one_the_sound_is_inside() {
        assert_eq!(playing(0).at(), 0);
        assert_eq!(playing(1_549).at(), 0);
        assert_eq!(playing(1_550).at(), 1);
        assert_eq!(playing(9_000).at(), 2);
    }

    #[test]
    fn a_reading_with_no_stream_yet_is_on_its_first_utterance() {
        assert_eq!(playing(0).at(), 0);
        assert_eq!(
            Reading {
                offsets: Vec::new(),
                ..playing(0)
            }
            .at(),
            0
        );
    }

    #[test]
    fn a_voice_is_read_with_the_home_directory_for_its_tilde() {
        let home = Path::new("/home/me");
        assert_eq!(
            voice_file("~/.varde/voices/v.onnx", home),
            Some(PathBuf::from("/home/me/.varde/voices/v.onnx"))
        );
        assert_eq!(
            voice_file("/voices/v.onnx", home),
            Some(PathBuf::from("/voices/v.onnx"))
        );
        assert_eq!(
            voice_file("~other/v.onnx", home),
            Some(PathBuf::from("~other/v.onnx"))
        );
        assert_eq!(voice_file("", home), None);
    }

    #[test]
    fn forward_and_back_land_on_an_utterance_start() {
        assert_eq!(playing(200).forward(), Seek::To(1_550));
        assert_eq!(playing(1_600).back(), Seek::To(0));
        assert_eq!(playing(3_100).back(), Seek::To(1_550));
    }

    #[test]
    fn back_at_the_first_stays_and_forward_at_the_last_ends() {
        assert_eq!(playing(200).back(), Seek::To(0));
        assert_eq!(playing(3_500).forward(), Seek::Ended);
    }

    #[test]
    fn a_seek_with_no_stream_yet_goes_nowhere() {
        let building = Reading {
            offsets: Vec::new(),
            ..playing(0)
        };
        assert_eq!(building.forward(), Seek::Nowhere);
        assert_eq!(building.back(), Seek::Nowhere);
    }

    #[test]
    fn the_duration_scale_is_the_reciprocal_of_the_speed() {
        assert!((duration_scale(1.00) - 1.00).abs() < 0.005);
        assert!((duration_scale(1.25) - 0.80).abs() < 0.005);
        assert!((duration_scale(0.90) - 1.11).abs() < 0.005);
        assert_eq!(duration_scale(0.0), 1.0);
    }

    #[test]
    fn only_a_markdown_buffer_has_a_transport() {
        let mut state = State {
            current_buffer: Some(std::path::PathBuf::from("/w/guide.md")),
            ..State::default()
        };
        assert_eq!(names(&state), [PLAY_PAUSE, PREVIOUS, NEXT, STOP, SPEED]);
        state.current_buffer = Some(std::path::PathBuf::from("/w/main.rs"));
        assert_eq!(transport(&state), []);
        state.current_buffer = None;
        assert_eq!(transport(&state), []);
    }

    #[test]
    fn the_play_control_turns_into_a_pause_while_a_reading_plays() {
        let mut state = State {
            current_buffer: Some(std::path::PathBuf::from("/w/guide.md")),
            ..State::default()
        };
        let glyph = |state: &State| transport(state)[0].glyph.clone();
        assert_eq!(glyph(&state), "\u{25ba}");
        state.reading = Some(Reading {
            utterances: utterances("One."),
            offsets: Vec::new(),
            at_ms: 0,
            paused: false,
            file: None,
        });
        assert_eq!(glyph(&state), "\u{25ae}");
        state.reading.as_mut().expect("a reading").paused = true;
        assert_eq!(glyph(&state), "\u{25ba}");
    }

    #[test]
    fn the_speed_control_says_the_speed() {
        let mut state = State {
            current_buffer: Some(std::path::PathBuf::from("/w/guide.md")),
            ..State::default()
        };
        state.speech.speed = 1.25;
        assert_eq!(transport(&state)[4].glyph, "1.25x");
    }

    #[test]
    fn the_speed_control_steps_the_ladder_and_wraps() {
        assert_eq!(next_speed(1.0), 1.25);
        assert_eq!(next_speed(1.2499999), 1.5);
        assert_eq!(next_speed(2.0), 0.75);
        assert_eq!(next_speed(3.0), 0.75);
    }

    fn names(state: &State) -> Vec<&'static str> {
        transport(state)
            .into_iter()
            .map(|chip| chip.action)
            .collect()
    }

    fn tones(state: &State) -> Vec<crate::Tone> {
        transport(state).into_iter().map(|chip| chip.tone).collect()
    }

    #[test]
    fn with_nothing_in_flight_only_play_and_speed_are_available() {
        use crate::Tone::{Dimmed, Plain};
        let mut state = State {
            current_buffer: Some(std::path::PathBuf::from("/w/guide.md")),
            ..State::default()
        };
        assert_eq!(tones(&state), [Plain, Dimmed, Dimmed, Dimmed, Plain]);
        state.reading = Some(Reading {
            utterances: utterances("One."),
            offsets: Vec::new(),
            at_ms: 0,
            paused: true,
            file: None,
        });
        assert_eq!(tones(&state), [Plain; 5]);
    }

    #[test]
    fn the_last_action_taken_is_lit() {
        use crate::Tone::{Dimmed, Lit, Plain};
        let mut state = State {
            current_buffer: Some(std::path::PathBuf::from("/w/guide.md")),
            transport_lit: Some(STOP),
            ..State::default()
        };
        assert_eq!(tones(&state), [Plain, Dimmed, Dimmed, Lit, Plain]);
        state.transport_lit = Some(PLAY_PAUSE);
        assert_eq!(tones(&state), [Lit, Dimmed, Dimmed, Dimmed, Plain]);
    }

    #[test]
    fn every_glyph_is_one_cell_in_every_font() {
        use unicode_width::UnicodeWidthChar;
        const EMOJI: [char; 8] = [
            '\u{25aa}', '\u{25ab}', '\u{25b6}', '\u{25c0}', '\u{25fb}', '\u{25fc}', '\u{25fd}',
            '\u{25fe}',
        ];
        let mut state = State {
            current_buffer: Some(std::path::PathBuf::from("/w/guide.md")),
            ..State::default()
        };
        let mut glyphs: Vec<String> = transport(&state).into_iter().map(|c| c.glyph).collect();
        state.reading = Some(Reading {
            utterances: utterances("One."),
            offsets: Vec::new(),
            at_ms: 0,
            paused: false,
            file: None,
        });
        glyphs.extend(transport(&state).into_iter().map(|c| c.glyph));
        for glyph in glyphs.iter().flat_map(|glyph| glyph.chars()) {
            assert_eq!(glyph.width(), Some(1), "{glyph:?}");
            let safe = glyph.is_ascii()
                || ('\u{a0}'..='\u{ff}').contains(&glyph)
                || (('\u{25a0}'..='\u{25ff}').contains(&glyph) && !EMOJI.contains(&glyph));
            assert!(safe, "{glyph:?} is not one cell in every font");
        }
    }

    #[test]
    fn nothing_to_say_is_no_utterances() {
        assert_eq!(utterances(""), []);
        assert_eq!(utterances("\n\n"), []);
    }

    #[test]
    fn the_wav_is_the_last_token_of_the_line_that_names_one() {
        assert_eq!(wrote("/tmp/a/1.wav\n"), Some("/tmp/a/1.wav"));
        assert_eq!(
            wrote("INFO:__main__:Wrote /tmp/a/1.wav\n"),
            Some("/tmp/a/1.wav")
        );
        assert_eq!(wrote("INFO:__main__:Loaded /voices/en_US.onnx"), None);
        assert_eq!(wrote(""), None);
        assert_eq!(wrote("\n"), None);
    }
}
