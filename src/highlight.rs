use std::sync::OnceLock;
use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keyword,
    Operator,
    String,
    Comment,
    Number,
    Constant,
    Function,
    Type,
    Property,
    Attribute,
    Punctuation,
    Markup,
    Invalid,
    Plain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub kind: Kind,
}

fn syntaxes() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(two_face::syntax::extra_no_newlines)
}

pub fn highlight(name: &str, source: &str) -> Vec<Vec<Token>> {
    let set = syntaxes();
    let syntax = name
        .rsplit_once('.')
        .and_then(|(_, extension)| set.find_syntax_by_extension(extension))
        .or_else(|| set.find_syntax_by_token(name));
    let Some(syntax) = syntax else {
        return plain(source);
    };

    let mut parse = ParseState::new(syntax);
    // syntect's parse_line reports only scope changes, so the stack must outlive each line
    let mut stack = ScopeStack::new();
    let mut lines = Vec::new();
    for line in source.split('\n') {
        let mut tokens = Vec::new();
        let Ok(ops) = parse.parse_line(line, set) else {
            push(&mut tokens, line, Kind::Plain);
            lines.push(tokens);
            continue;
        };
        for (text, op) in ScopeRegionIterator::new(&ops, line) {
            if stack.apply(op).is_err() {
                continue;
            }
            if !text.is_empty() {
                push(&mut tokens, text, classify(&stack));
            }
        }
        lines.push(tokens);
    }
    lines
}

pub fn plain(source: &str) -> Vec<Vec<Token>> {
    source.split('\n').map(plain_line).collect()
}

fn plain_line(line: &str) -> Vec<Token> {
    match line.is_empty() {
        true => Vec::new(),
        false => vec![Token {
            text: line.to_string(),
            kind: Kind::Plain,
        }],
    }
}

pub fn carried(
    mut tokens: Vec<Vec<Token>>,
    marks: Vec<usize>,
    source: &str,
) -> (Vec<Vec<Token>>, Vec<usize>) {
    let lines: Vec<&str> = source.split('\n').collect();
    let holds = |tokens: &[Token], line: &str| {
        let mut rest = line;
        tokens
            .iter()
            .all(|token| match rest.strip_prefix(token.text.as_str()) {
                Some(after) => {
                    rest = after;
                    true
                }
                None => false,
            })
            && rest.is_empty()
    };
    let both = tokens.len().min(lines.len());
    let above = (0..both)
        .take_while(|&index| holds(&tokens[index], lines[index]))
        .count();
    let below = (1..=both - above)
        .take_while(|&back| holds(&tokens[tokens.len() - back], lines[lines.len() - back]))
        .count();
    let end = tokens.len() - below;
    let marks = marks
        .into_iter()
        .filter_map(|line| match line {
            line if line <= above => Some(line),
            line if line > end => Some(line - end + lines.len() - below),
            _ => None,
        })
        .collect();
    tokens.splice(
        above..end,
        lines[above..lines.len() - below]
            .iter()
            .map(|line| plain_line(line)),
    );
    (tokens, marks)
}

fn push(tokens: &mut Vec<Token>, text: &str, kind: Kind) {
    match tokens.last_mut() {
        Some(last) if last.kind == kind => last.text.push_str(text),
        _ => tokens.push(Token {
            text: text.to_string(),
            kind,
        }),
    }
}

fn classify(stack: &ScopeStack) -> Kind {
    let mut weakest = Kind::Plain;
    for scope in stack.scopes.iter().rev() {
        match scope_kind(&scope.build_string()) {
            Kind::Plain => {}
            Kind::Punctuation => weakest = Kind::Punctuation,
            claimed => return claimed,
        }
    }
    weakest
}

/// keep keyword.operator above the keyword arm: grammars put `=` and `+` in the keyword family
fn scope_kind(name: &str) -> Kind {
    let rest = name
        .split_once('.')
        .map(|(_, rest)| rest)
        .unwrap_or_default();
    match name.split('.').next().unwrap_or_default() {
        "comment" => Kind::Comment,
        "string" => Kind::String,
        "punctuation" => Kind::Punctuation,
        "invalid" => Kind::Invalid,
        "markup" => Kind::Markup,
        "keyword" if rest.starts_with("operator") => Kind::Operator,
        "keyword" | "storage" => Kind::Keyword,
        "constant" if rest.starts_with("numeric") => Kind::Number,
        "constant" => Kind::Constant,
        "entity" if rest.starts_with("name.function") => Kind::Function,
        "entity"
            if rest.starts_with("name.tag")
                || rest.starts_with("name.table")
                || rest.starts_with("name.section") =>
        {
            Kind::Markup
        }
        "entity" if rest.starts_with("other.attribute-name") => Kind::Attribute,
        "entity" => Kind::Type,
        "support" if rest.starts_with("function") => Kind::Function,
        "support" if rest.starts_with("type") || rest.starts_with("class") => Kind::Type,
        "support" => Kind::Constant,
        "variable" if rest.starts_with("annotation") => Kind::Attribute,
        "variable"
            if rest.starts_with("parameter")
                || rest.starts_with("other.property")
                || rest.starts_with("object.property") =>
        {
            Kind::Property
        }
        "variable" => Kind::Plain,
        _ => Kind::Plain,
    }
}

#[cfg(test)]
mod tests {
    use super::{carried, highlight, plain, scope_kind, Kind, Token};

    #[test]
    fn an_edit_keeps_the_colours_of_every_line_it_left_alone() {
        let before = highlight("main.rs", "fn a() {}\nlet x = 1;\nfn b() {}");
        let (after, _) = carried(
            before.clone(),
            Vec::new(),
            "fn a() {}\nlet x = 12;\nlet y = 2;\nfn b() {}",
        );
        assert_eq!(after[0], before[0]);
        assert_eq!(after[1..3], plain("let x = 12;\nlet y = 2;")[..]);
        assert_eq!(after[3], before[2]);
    }

    #[test]
    fn a_run_mark_moves_with_the_line_it_stands_on() {
        let before = "fn a() {}\nlet x = 1;\nfn b() {}";
        let marks = |after| carried(highlight("main.rs", before), vec![1, 2, 3], after).1;
        assert_eq!(
            marks("fn a() {}\nlet x = 12;\nlet y = 2;\nfn b() {}"),
            [1, 4]
        );
        assert_eq!(marks("fn b() {}"), [1]);
        assert_eq!(marks(before), [1, 2, 3]);
    }

    #[test]
    fn carried_tokens_hold_the_text_as_it_is_now() {
        let text = |tokens: &[Vec<Token>]| {
            tokens
                .iter()
                .map(|line| {
                    line.iter()
                        .map(|token| token.text.as_str())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        for (before, after) in [
            ("a\nb\nc", "a\nc"),
            ("a\na\na", "a\na"),
            ("a\na", "a\na\na"),
            ("fn a() {}\n", ""),
            ("", "fn a() {}"),
            ("x", "x"),
        ] {
            assert_eq!(
                text(&carried(highlight("main.rs", before), Vec::new(), after).0),
                after,
                "{before:?} to {after:?}"
            );
        }
    }
    use syntect::easy::ScopeRegionIterator;
    use syntect::parsing::{ParseState, ScopeStack};

    fn kind_of(name: &str, source: &str, needle: &str) -> Kind {
        highlight(name, source)
            .into_iter()
            .flatten()
            .find(|token| token.text.trim() == needle)
            .unwrap_or_else(|| panic!("no token {needle:?} in {source:?}"))
            .kind
    }

    fn families(name: &str, source: &str) -> Vec<String> {
        let set = super::syntaxes();
        let syntax = set
            .find_syntax_by_extension(name.rsplit_once('.').expect("an extension").1)
            .expect("a known language");
        let mut parse = ParseState::new(syntax);
        let mut seen = Vec::new();
        for line in source.split('\n') {
            let ops = parse.parse_line(line, set).expect("a parsed line");
            let mut stack = ScopeStack::new();
            for (_, op) in ScopeRegionIterator::new(&ops, line) {
                stack.apply(op).expect("a valid scope operation");
                for scope in &stack.scopes {
                    let family = scope
                        .build_string()
                        .split('.')
                        .next()
                        .unwrap_or_default()
                        .to_string();
                    if !seen.contains(&family) {
                        seen.push(family);
                    }
                }
            }
        }
        seen
    }

    #[test]
    fn tokens_are_grouped_by_line() {
        let lines = highlight("a.rs", "let x = 1;\n\n/* note\n   still */");
        let text: Vec<String> = lines
            .iter()
            .map(|tokens| tokens.iter().map(|token| token.text.as_str()).collect())
            .collect();
        assert_eq!(text, ["let x = 1;", "", "/* note", "   still */"]);
        assert_eq!(lines[1], Vec::new());
        assert_eq!(lines[3][0].kind, Kind::Comment, "{:?}", lines[3]);
    }

    #[test]
    fn storage_words_count_as_keywords() {
        assert_eq!(kind_of("a.rs", "let x = 1;", "let"), Kind::Keyword);
        assert_eq!(kind_of("a.rs", "fn main() {}", "fn"), Kind::Keyword);
    }

    #[test]
    fn a_string_arrives_whole_including_its_quotes() {
        assert_eq!(kind_of("a.rs", "let s = \"hi\";", "\"hi\""), Kind::String);
    }

    #[test]
    fn numbers_are_not_keywords() {
        assert_eq!(kind_of("a.rs", "let x = 42;", "42"), Kind::Number);
    }

    #[test]
    fn a_language_name_works_as_well_as_a_file_name() {
        assert_eq!(kind_of("rust", "fn main() {}", "fn"), Kind::Keyword);
    }

    #[test]
    fn an_unknown_language_name_renders_as_plain() {
        let tokens = highlight("not-a-real-language", "fn main() {}");
        assert!(
            tokens
                .iter()
                .flatten()
                .all(|token| token.kind == Kind::Plain),
            "{tokens:?}"
        );
    }

    #[test]
    fn operators_are_not_keywords() {
        assert_eq!(kind_of("a.rs", "let x = 1 + 2;", "="), Kind::Operator);
        assert_eq!(kind_of("a.rs", "let x = 1 + 2;", "+"), Kind::Operator);
    }

    #[test]
    fn assignment_arithmetic_and_comparison_operators_in_two_languages() {
        for (name, source) in [
            ("a.rs", "if a == b { c += 1; }"),
            ("a.js", "if (a === b) { c += 1; }"),
        ] {
            assert_eq!(kind_of(name, source, "if"), Kind::Keyword, "{name}");
            assert_eq!(kind_of(name, source, "+="), Kind::Operator, "{name}");
            let comparison = if name.ends_with(".rs") { "==" } else { "===" };
            assert_eq!(kind_of(name, source, comparison), Kind::Operator, "{name}");
        }
    }

    const FAMILIES: [(&str, Kind); 15] = [
        ("comment", Kind::Comment),
        ("constant", Kind::Constant),
        ("embedded", Kind::Plain),
        ("entity", Kind::Type),
        ("invalid", Kind::Invalid),
        ("keyword", Kind::Keyword),
        ("markup", Kind::Markup),
        ("meta", Kind::Plain),
        ("punctuation", Kind::Punctuation),
        ("source", Kind::Plain),
        ("storage", Kind::Keyword),
        ("string", Kind::String),
        ("support", Kind::Constant),
        ("text", Kind::Plain),
        ("variable", Kind::Plain),
    ];

    #[test]
    fn every_convention_family_maps_to_a_kind() {
        for (family, kind) in FAMILIES {
            assert_eq!(scope_kind(family), kind, "{family}");
        }
    }

    #[test]
    fn no_grammar_produces_a_family_the_table_does_not_name() {
        let corpus = [
            (
                "a.rs",
                "#[derive(Debug)]\nfn f(x: u8) -> Vec<String> { let s = \"hi\"; }",
            ),
            (
                "a.js",
                "// note\nconfig.debug = false;\nfunction f(a) { return a + 1; }",
            ),
            ("a.py", "def f(a, b): return None"),
            ("a.go", "x := 09"),
            ("a.java", "@Override public class A { }"),
            ("a.css", "a { color: red; }"),
            ("a.yaml", "key: value"),
            ("index.html", "<div class=\"a\">hi</div>"),
            ("README.md", "A **bold** word."),
        ];
        for (name, source) in corpus {
            for family in families(name, source) {
                assert!(
                    FAMILIES.iter().any(|(known, _)| *known == family),
                    "{name} produced the unhandled family {family:?}"
                );
            }
        }
    }

    #[test]
    fn a_function_call_is_a_function() {
        assert_eq!(
            kind_of("a.rs", "let n = compute(1);", "compute"),
            Kind::Function
        );
    }

    #[test]
    fn language_literals_are_constants_and_numbers_stay_numbers() {
        assert_eq!(kind_of("a.rs", "let ok = true;", "true"), Kind::Constant);
        assert_eq!(
            kind_of("a.js", "const ok = false;", "false"),
            Kind::Constant
        );
        assert_eq!(kind_of("a.py", "x = None", "None"), Kind::Constant);
        assert_eq!(kind_of("a.rs", "let x = 42;", "42"), Kind::Number);
    }

    #[test]
    fn punctuation_is_the_weakest_kind() {
        let source = "let point: Pair = make(1, 2);";
        assert_eq!(kind_of("a.rs", source, ":"), Kind::Punctuation);
        assert_eq!(kind_of("a.rs", source, ","), Kind::Punctuation);
        assert_eq!(kind_of("a.rs", "let s = \"hi\";", "\"hi\""), Kind::String);
        assert_eq!(kind_of("a.rs", "// a note", "// a note"), Kind::Comment);
        assert_eq!(
            kind_of("README.md", "A **bold** word.", "**bold**"),
            Kind::Markup
        );
    }

    #[test]
    fn an_annotation_is_an_attribute() {
        assert_eq!(
            kind_of("a.rs", "#[derive(Debug)]", "derive"),
            Kind::Attribute
        );
        assert_eq!(
            kind_of("a.java", "@Override public class A { }", "Override"),
            Kind::Attribute
        );
    }

    #[test]
    fn a_tag_is_markup_and_its_attribute_name_is_an_attribute() {
        let source = "<div class=\"a\">hi</div>";
        assert_eq!(kind_of("index.html", source, "div"), Kind::Markup);
        assert_eq!(kind_of("index.html", source, "class"), Kind::Attribute);
    }

    #[test]
    fn a_mapping_key_is_markup() {
        assert_eq!(kind_of("a.yaml", "key: value", "key"), Kind::Markup);
    }

    #[test]
    fn the_extended_set_colours_the_languages_the_default_set_missed() {
        let corpus = [
            (
                "a.ts",
                "const total: number = add(1, 2);",
                "const",
                Kind::Keyword,
            ),
            (
                "a.tsx",
                "const el = <Box name=\"a\" />;",
                "name",
                Kind::Attribute,
            ),
            (
                "a.jsx",
                "const el = <Box name=\"a\" />;",
                "name",
                Kind::Attribute,
            ),
            (
                "A.vue",
                "<template><div class=\"a\">hi</div></template>",
                "class",
                Kind::Attribute,
            ),
            (
                "A.svelte",
                "<script>let n = 1;</script>",
                "let",
                Kind::Keyword,
            ),
            ("A.kt", "fun main() { val n = 1 }", "fun", Kind::Keyword),
            (
                "A.swift",
                "func main() { let n = 1 }",
                "func",
                Kind::Keyword,
            ),
            (
                "a.zig",
                "const std = @import(\"std\");",
                "\"std\"",
                Kind::String,
            ),
            (
                "a.dart",
                "void main() { var n = 1; }",
                "main",
                Kind::Function,
            ),
            ("config.toml", "theme = \"dark\"", "theme", Kind::Markup),
            (
                "main.tf",
                "resource \"aws_s3_bucket\" \"b\" {}",
                "resource",
                Kind::Keyword,
            ),
            (
                "default.nix",
                "{ pkgs }: pkgs.hello",
                "pkgs",
                Kind::Property,
            ),
            ("a.ex", "defmodule A do end", "defmodule", Kind::Keyword),
            (
                "a.proto",
                "message A { string name = 1; }",
                "message",
                Kind::Keyword,
            ),
            ("Dockerfile", "FROM alpine:3.19", "FROM", Kind::Keyword),
            (
                "a.graphql",
                "type Query { name: String }",
                "Query",
                Kind::Type,
            ),
            ("a.scss", "$c: red; a { color: $c; }", "red", Kind::Constant),
            (
                "typescript",
                "const total: number = f();",
                "number",
                Kind::Type,
            ),
            ("toml", "theme = \"dark\"", "\"dark\"", Kind::String),
        ];
        for (name, source, anchor, kind) in corpus {
            assert_eq!(kind_of(name, source, anchor), kind, "{name}");
        }
    }
}
