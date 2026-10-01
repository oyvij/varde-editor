use crate::editor::Buffer;
use crate::{Place, State};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeBounds;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    Open,
    Folded,
}

pub fn blocks(source: &str) -> Vec<Block> {
    let indents: Vec<Option<usize>> = source.split('\n').map(indent).collect();
    (0..indents.len())
        .filter_map(|index| block_at(&indents, index))
        .collect()
}

fn block_at(indents: &[Option<usize>], index: usize) -> Option<Block> {
    let indent = (*indents.get(index)?)?;
    let mut last = None;
    for (below, under) in indents.iter().enumerate().skip(index + 1) {
        match under {
            None => continue,
            Some(deeper) if *deeper > indent => last = Some(below + 1),
            Some(_) => break,
        }
    }
    last.map(|to| Block {
        from: index + 1,
        to,
    })
}

pub(crate) fn indent(line: &str) -> Option<usize> {
    (!line.trim().is_empty()).then(|| line.chars().take_while(|c| c.is_whitespace()).count())
}

pub fn toggle(buffer: &mut Buffer, all: bool) {
    let blocks = blocks(buffer.shown());
    if all {
        buffer.folded = match buffer.folded.is_empty() {
            true => blocks.iter().map(|block| block.from).collect(),
            false => Vec::new(),
        };
        return;
    }
    let Some(block) = blocks
        .iter()
        .filter(|block| block.from <= buffer.line && buffer.line <= block.to)
        .max_by_key(|block| block.from)
    else {
        return;
    };
    if buffer.folded.contains(&block.from) {
        buffer.folded.retain(|folded| *folded != block.from);
        return;
    }
    buffer.folded.push(block.from);
    buffer.go_to_place(Place {
        line: block.from,
        column: 1,
    });
}

pub fn hidden(state: &State) -> BTreeSet<usize> {
    let Some(buffer) = folding(state).filter(|buffer| !buffer.folded.is_empty()) else {
        return BTreeSet::new();
    };
    buffer
        .folded
        .iter()
        .filter_map(|from| block_at(buffer.indents(), from.checked_sub(1)?))
        .flat_map(|block| block.from + 1..=block.to)
        .collect()
}

pub fn toggles(state: &State, lines: impl RangeBounds<usize>) -> BTreeMap<usize, Toggle> {
    let Some(buffer) = folding(state) else {
        return BTreeMap::new();
    };
    let indents = buffer.indents();
    buffer
        .lines_within(lines)
        .filter(|(number, _)| {
            indents[number - 1].is_some_and(|depth| {
                indents[*number..]
                    .iter()
                    .flatten()
                    .next()
                    .is_some_and(|below| *below > depth)
            })
        })
        .map(|(number, _)| {
            let toggle = match buffer.folded.contains(&number) {
                true => Toggle::Folded,
                false => Toggle::Open,
            };
            (number, toggle)
        })
        .collect()
}

fn folding(state: &State) -> Option<&Buffer> {
    match state.diff.is_some() || state.walking.is_some() || crate::previewing(state) {
        true => None,
        false => crate::current_buffer(state),
    }
}

#[cfg(test)]
mod tests {
    use super::{blocks, Block};

    fn spans(source: &str) -> Vec<(usize, usize)> {
        blocks(source)
            .into_iter()
            .map(|Block { from, to }| (from, to))
            .collect()
    }

    #[test]
    fn a_function_body_is_a_block() {
        assert_eq!(spans("fn main() {\n    go();\n    stop();\n}\n"), [(1, 3)]);
    }

    #[test]
    fn an_indented_language_folds_by_the_same_rule() {
        assert_eq!(spans("def f():\n    go()\n    stop()\n"), [(1, 3)]);
    }

    #[test]
    fn a_nested_block_is_its_own_block_inside_the_one_around_it() {
        assert_eq!(
            spans("fn main() {\n    if ok {\n        go();\n    }\n}\n"),
            [(1, 4), (2, 3)]
        );
    }

    #[test]
    fn a_blank_line_inside_a_block_belongs_to_it_and_a_trailing_one_does_not() {
        assert_eq!(
            spans("fn one() {\n    go();\n\n    stop();\n}\n\nfn two() {\n    go();\n}\n"),
            [(1, 4), (7, 8)]
        );
    }

    #[test]
    fn lines_at_one_depth_hold_no_block() {
        assert_eq!(spans("use std::fs;\nuse std::io;\n"), []);
    }
}
