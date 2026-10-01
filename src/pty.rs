use anyhow::Result;
use portable_pty::{CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use varde::{keys, mouse, queries};

#[derive(Default)]
struct Answers(Vec<u8>);

impl vt100::Callbacks for Answers {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        _i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        if let Some(reply) = queries::reply(i1, params, c, screen.cursor_position()) {
            self.0.extend_from_slice(&reply);
        }
    }
}

pub struct Pane {
    parser: vt100::Parser<Answers>,
    writer: Box<dyn Write + Send>,
    output: Receiver<Vec<u8>>,
    master: Box<dyn MasterPty + Send>,
    pid: Option<u32>,
    pub alive: bool,
    pub spoken: bool,
    printed: bool,
    quiet: u8,
}

const QUIET: u8 = 5;

impl Pane {
    pub fn spawn(
        argv: &[String],
        cwd: &Path,
        env: &BTreeMap<String, String>,
        rows: u16,
        cols: u16,
    ) -> Result<Self> {
        // vt100 panics on a zero-sized grid and underflows on a one-row grid, so clamp to two rows
        let (rows, cols) = (rows.max(2), cols.max(1));
        let pty = NativePtySystem::default().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut builder = match argv.split_first() {
            Some((program, arguments)) => {
                let mut builder = CommandBuilder::new(program);
                builder.args(arguments);
                builder
            }
            None => CommandBuilder::new_default_prog(),
        };
        builder.cwd(cwd);
        for (name, value) in queries::CHILD_ENV {
            match value {
                Some(value) => builder.env(name, value),
                // Remove one at a time: env_clear would also drop PATH and HOME
                None => builder.env_remove(name),
            }
        }
        for (name, value) in env {
            builder.env(name, value);
        }
        let mut child = pty.slave.spawn_command(builder)?;
        let pid = child.process_id();
        drop(pty.slave);

        let mut reader = pty.master.try_clone_reader()?;
        let (sender, output) = channel();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 || sender.send(buffer[..read].to_vec()).is_err() {
                    break;
                }
            }
            let _ = child.wait();
        });

        Ok(Self {
            parser: vt100::Parser::new_with_callbacks(rows, cols, 2000, Answers::default()),
            writer: pty.master.take_writer()?,
            output,
            master: pty.master,
            pid,
            alive: true,
            spoken: false,
            printed: false,
            quiet: 0,
        })
    }

    pub fn drain(&mut self) -> bool {
        let mut arrived = false;
        loop {
            match self.output.try_recv() {
                Ok(chunk) => {
                    self.printed = true;
                    self.quiet = 0;
                    arrived = true;
                    self.parser.process(&chunk);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.alive = false;
                    arrived = true;
                    break;
                }
            }
        }
        if self.printed && !arrived {
            self.quiet = (self.quiet + 1).min(QUIET);
        }
        self.spoken |= self.printed && (self.screen().bracketed_paste() || self.quiet == QUIET);
        let answers = std::mem::take(&mut self.parser.callbacks_mut().0);
        if !answers.is_empty() {
            let _ = self.writer.write_all(&answers);
            let _ = self.writer.flush();
        }
        arrived
    }

    pub fn cwd(&self) -> Option<PathBuf> {
        let pid = self.pid?;
        if cfg!(target_os = "macos") {
            let out = std::process::Command::new("lsof")
                .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
                .output()
                .ok()?;
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .find_map(|line| line.strip_prefix('n'))
                .map(PathBuf::from)
        } else {
            std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
        }
    }

    pub fn busy(&self) -> bool {
        match (self.master.process_group_leader(), self.pid) {
            (Some(leader), Some(pid)) => leader != pid as libc::pid_t,
            _ => false,
        }
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    pub fn send(&mut self, bytes: &[u8]) {
        self.parser.screen_mut().set_scrollback(0);
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    pub fn scroll(&mut self, up: bool) {
        let screen = self.parser.screen_mut();
        let at = screen.scrollback();
        screen.set_scrollback(if up { at + 3 } else { at.saturating_sub(3) });
    }

    pub fn scrolled_back(&self) -> bool {
        self.screen().scrollback() > 0
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        // vt100 panics on a zero-sized grid and underflows on a one-row grid, so clamp to two rows
        let (rows, cols) = (rows.max(2), cols.max(1));
        if (rows, cols) == self.parser.screen().size() {
            return;
        }
        self.parser.screen_mut().set_size(rows, cols);
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    pub fn bracketed_paste(&self) -> keys::Paste {
        if self.screen().bracketed_paste() {
            keys::Paste::Bracketed
        } else {
            keys::Paste::Bare
        }
    }

    /// UTF-8 mouse mode 1005 maps to none: its bytes diverge from legacy past column 95
    pub fn mouse_encoding(&self) -> mouse::Encoding {
        match self.screen().mouse_protocol_mode() {
            vt100::MouseProtocolMode::None => mouse::Encoding::None,
            _ => match self.screen().mouse_protocol_encoding() {
                vt100::MouseProtocolEncoding::Sgr => mouse::Encoding::Sgr,
                vt100::MouseProtocolEncoding::Default => mouse::Encoding::Legacy,
                vt100::MouseProtocolEncoding::Utf8 => mouse::Encoding::None,
            },
        }
    }
}
