pub const CHILD_ENV: &[(&str, Option<&str>)] = &[
    ("TERM", Some("xterm-256color")),
    ("COLORTERM", Some("truecolor")),
    ("TERM_PROGRAM", None),
    ("TERM_PROGRAM_VERSION", None),
    ("TERM_SESSION_ID", None),
    ("TERMINAL_EMULATOR", None),
    ("TERMINFO", None),
    ("TERMCAP", None),
    ("LC_TERMINAL", None),
    ("LC_TERMINAL_VERSION", None),
    ("XTERM_VERSION", None),
    ("XTERM_SHELL", None),
    ("XTERM_LOCALE", None),
    ("WINDOWID", None),
    ("KITTY_WINDOW_ID", None),
    ("KITTY_PID", None),
    ("KITTY_LISTEN_ON", None),
    ("KITTY_INSTALLATION_DIR", None),
    ("KITTY_PUBLIC_KEY", None),
    ("KITTY_SHELL_INTEGRATION", None),
    ("ITERM_SESSION_ID", None),
    ("ITERM_PROFILE", None),
    ("ITERM2_SQUELCH_MARK", None),
    ("WEZTERM_PANE", None),
    ("WEZTERM_EXECUTABLE", None),
    ("WEZTERM_EXECUTABLE_DIR", None),
    ("WEZTERM_CONFIG_FILE", None),
    ("WEZTERM_CONFIG_DIR", None),
    ("WEZTERM_UNIX_SOCKET", None),
    ("ALACRITTY_WINDOW_ID", None),
    ("ALACRITTY_SOCKET", None),
    ("ALACRITTY_LOG", None),
    ("GHOSTTY_BIN_DIR", None),
    ("GHOSTTY_RESOURCES_DIR", None),
    ("GHOSTTY_SHELL_FEATURES", None),
    ("VTE_VERSION", None),
    ("TERMUX_VERSION", None),
    ("KONSOLE_VERSION", None),
    ("KONSOLE_PROFILE_NAME", None),
    ("KONSOLE_DBUS_SESSION", None),
    ("KONSOLE_DBUS_SERVICE", None),
    ("KONSOLE_DBUS_WINDOW", None),
    ("WT_SESSION", None),
    ("WT_PROFILE_ID", None),
    ("TMUX", None),
    ("TMUX_PANE", None),
    ("STY", None),
    ("ZELLIJ", None),
    ("ZELLIJ_SESSION_NAME", None),
    ("ZELLIJ_PANE_ID", None),
];

pub fn reply(
    intermediate: Option<u8>,
    params: &[&[u16]],
    final_byte: char,
    cursor: (u16, u16),
) -> Option<Vec<u8>> {
    let first = params.first().and_then(|p| p.first().copied());
    match (intermediate, final_byte, first) {
        (None, 'n', Some(6)) => {
            let (row, column) = (cursor.0 + 1, cursor.1 + 1);
            Some(format!("\x1b[{row};{column}R").into_bytes())
        }
        (Some(b'?'), 'n', Some(6)) => {
            let (row, column) = (cursor.0 + 1, cursor.1 + 1);
            Some(format!("\x1b[?{row};{column}R").into_bytes())
        }
        (None, 'n', Some(5)) => Some(b"\x1b[0n".to_vec()),
        (Some(b'>'), 'q', None | Some(0)) => {
            Some(format!("\x1bP>|VARDE({})\x1b\\", env!("CARGO_PKG_VERSION")).into_bytes())
        }
        (None, 'c', None | Some(0)) => Some(b"\x1b[?62;22c".to_vec()),
        (Some(b'>'), 'c', None | Some(0)) => Some(b"\x1b[>1;10;0c".to_vec()),
        (Some(b'?'), 'm', resource @ (None | Some(0 | 1 | 2 | 4))) => {
            let resource = resource.unwrap_or(0);
            Some(format!("\x1b[>{resource};0m").into_bytes())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{reply, CHILD_ENV};

    fn answer(i1: Option<u8>, params: &[&[u16]], final_byte: char, cursor: (u16, u16)) -> String {
        String::from_utf8(reply(i1, params, final_byte, cursor).expect("a reply")).unwrap()
    }

    #[test]
    fn a_cursor_position_query_is_answered_with_the_cursor() {
        assert_eq!(answer(None, &[&[6]], 'n', (11, 41)), "\x1b[12;42R");
    }

    #[test]
    fn the_private_cursor_position_query_is_answered_privately() {
        assert_eq!(answer(Some(b'?'), &[&[6]], 'n', (11, 41)), "\x1b[?12;42R");
    }

    #[test]
    fn a_primary_device_attributes_query_is_answered_with_an_identity() {
        assert_eq!(answer(None, &[], 'c', (0, 0)), "\x1b[?62;22c");
        assert_eq!(answer(None, &[&[0]], 'c', (0, 0)), "\x1b[?62;22c");
    }

    #[test]
    fn a_secondary_device_attributes_query_is_answered() {
        assert_eq!(answer(Some(b'>'), &[], 'c', (0, 0)), "\x1b[>1;10;0c");
        assert_eq!(answer(Some(b'>'), &[&[0]], 'c', (0, 0)), "\x1b[>1;10;0c");
    }

    #[test]
    fn a_status_query_is_answered_that_the_terminal_is_working() {
        assert_eq!(answer(None, &[&[5]], 'n', (0, 0)), "\x1b[0n");
    }

    #[test]
    fn a_version_query_is_answered_with_vardes_own_name_and_version() {
        for params in [&[&[0u16][..]][..], &[]] {
            let answer = answer(Some(b'>'), params, 'q', (0, 0));
            assert_eq!(
                answer,
                format!("\x1bP>|VARDE({})\x1b\\", env!("CARGO_PKG_VERSION"))
            );
            assert!(answer.starts_with("\x1bP>|VARDE("), "{answer:?}");
            assert!(answer.ends_with(")\x1b\\"), "{answer:?}");
        }
    }

    #[test]
    fn a_modifier_reporting_query_is_answered_with_a_refusal() {
        assert_eq!(answer(Some(b'?'), &[&[4]], 'm', (0, 0)), "\x1b[>4;0m");
        assert_eq!(answer(Some(b'?'), &[&[1]], 'm', (0, 0)), "\x1b[>1;0m");
    }

    #[test]
    fn a_modifier_reporting_query_without_a_resource_refuses_the_first_one() {
        assert_eq!(answer(Some(b'?'), &[], 'm', (0, 0)), "\x1b[>0;0m");
        assert!(reply(Some(b'?'), &[&[99]], 'm', (0, 0)).is_none());
    }

    #[test]
    fn the_kitty_keyboard_query_is_refused_by_answering_what_it_is_paired_with() {
        assert!(reply(Some(b'?'), &[], 'u', (0, 0)).is_none());
    }

    #[test]
    fn a_sequence_that_is_not_a_query_yields_no_reply() {
        assert!(reply(None, &[&[4]], 'i', (3, 7)).is_none());
        assert!(reply(Some(b'>'), &[&[1]], 'u', (3, 7)).is_none());
        assert!(reply(Some(b'?'), &[&[1049]], 's', (3, 7)).is_none());
        assert!(reply(None, &[&[1]], 'q', (3, 7)).is_none());
        assert!(reply(Some(b' '), &[&[2]], 'q', (3, 7)).is_none());
        assert!(reply(Some(b'>'), &[&[1]], 'q', (3, 7)).is_none());
        assert!(reply(Some(b'?'), &[&[5]], 'n', (3, 7)).is_none());
    }
    #[test]
    fn the_child_is_told_the_terminal_varde_actually_renders() {
        assert!(CHILD_ENV.contains(&("TERM", Some("xterm-256color"))));
        assert!(CHILD_ENV.contains(&("COLORTERM", Some("truecolor"))));
    }

    #[test]
    fn every_other_variable_is_removed_rather_than_given_a_made_up_value() {
        for (name, value) in CHILD_ENV {
            assert!(
                matches!(*name, "TERM" | "COLORTERM") || value.is_none(),
                "{name} claims a value Varde cannot answer for"
            );
        }
    }

    #[test]
    fn no_emulator_the_user_may_have_launched_varde_from_leaves_a_marker() {
        for name in [
            "TERM_PROGRAM",
            "TERM_PROGRAM_VERSION",
            "TERMINFO",
            "XTERM_VERSION",
            "KITTY_WINDOW_ID",
            "ITERM_SESSION_ID",
            "WEZTERM_PANE",
            "ALACRITTY_WINDOW_ID",
            "GHOSTTY_RESOURCES_DIR",
            "VTE_VERSION",
            "KONSOLE_VERSION",
            "WT_SESSION",
            "TMUX",
        ] {
            assert!(
                CHILD_ENV.contains(&(name, None)),
                "{name} still reaches the child"
            );
        }
    }

    #[test]
    fn no_variable_is_named_twice() {
        let mut names: Vec<_> = CHILD_ENV.iter().map(|(name, _)| *name).collect();
        let all = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all, "a variable is both set and removed");
    }
}
