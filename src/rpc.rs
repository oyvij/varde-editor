use anyhow::Result;
use lsp_server::Message;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

const CONNECT: std::time::Duration = std::time::Duration::from_secs(10);

pub struct Server {
    child: Child,
    outgoing: Sender<String>,
    incoming: Receiver<String>,
    pub alive: bool,
}

impl Server {
    pub fn spawn(command: &str, args: &[String], cwd: &Path, log: Option<File>) -> Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // This process's stderr is the TUI's screen, so a server's log must never inherit it
            .stderr(log.map_or_else(Stdio::null, Stdio::from))
            .spawn()?;
        let stdout = child.stdout.take().expect("a piped stdout");
        let stdin = child.stdin.take().expect("a piped stdin");
        let (sender, incoming) = channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(message)) = Message::read(&mut reader) {
                let json = serde_json::to_string(&message).expect("a parsed message re-serialises");
                if sender.send(json).is_err() {
                    break;
                }
            }
        });
        let (outgoing, unsent) = channel::<String>();
        std::thread::spawn(move || {
            let mut stdin = BufWriter::new(stdin);
            for json in unsent {
                let written = serde_json::from_str::<Message>(&json)
                    .ok()
                    .map(|message| message.write(&mut stdin).and_then(|()| stdin.flush()));
                if !matches!(written, Some(Ok(()))) {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            outgoing,
            incoming,
            alive: true,
        })
    }

    pub fn send(&mut self, json: String) {
        if self.outgoing.send(json).is_err() {
            self.alive = false;
        }
    }

    pub fn drain(&mut self) -> Vec<String> {
        let mut arrived = Vec::new();
        loop {
            match self.incoming.try_recv() {
                Ok(json) => arrived.push(json),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.alive = false;
                    break;
                }
            }
        }
        arrived
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct Adapter {
    child: Option<Child>,
    outgoing: Sender<String>,
    incoming: Receiver<String>,
    pub alive: bool,
}

impl Adapter {
    pub fn spawn(
        command: &str,
        args: &[String],
        cwd: &Path,
        log: Option<File>,
    ) -> std::io::Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(log.map_or_else(Stdio::null, Stdio::from))
            .spawn()?;
        let stdout = child.stdout.take().expect("a piped stdout");
        let stdin = child.stdin.take().expect("a piped stdin");
        Ok(Self::over(Some(child), stdout, Box::new(stdin)))
    }

    pub fn connect(
        command: &str,
        args: &[String],
        cwd: &Path,
        log: Option<File>,
        port: u16,
    ) -> std::io::Result<Receiver<std::io::Result<Self>>> {
        let stdout = match &log {
            Some(file) => Stdio::from(file.try_clone()?),
            None => Stdio::null(),
        };
        let child = Command::new(command)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(log.map_or_else(Stdio::null, Stdio::from))
            .spawn()?;
        Ok(Self::dial(port, Some(child)))
    }

    pub fn dial(port: u16, mut child: Option<Child>) -> Receiver<std::io::Result<Self>> {
        let (sender, connected) = channel();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            // localhost, not one address: an adapter may listen on either loopback
            let stream = loop {
                match TcpStream::connect(("localhost", port)) {
                    Ok(stream) => break Ok(stream),
                    Err(error)
                        if started.elapsed() > CONNECT
                            || child
                                .as_mut()
                                .is_some_and(|child| !matches!(child.try_wait(), Ok(None))) =>
                    {
                        break Err(error)
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
                }
            };
            let adapter = stream.and_then(|stream| Ok((stream.try_clone()?, stream)));
            let _ = sender.send(match adapter {
                Ok((reader, writer)) => Ok(Self::over(child, reader, Box::new(writer))),
                Err(error) => {
                    if let Some(child) = child.as_mut() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    Err(error)
                }
            });
        });
        connected
    }

    fn over(
        child: Option<Child>,
        reader: impl std::io::Read + Send + 'static,
        writer: Box<dyn Write + Send>,
    ) -> Self {
        let (sender, incoming) = channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            while let Some(json) = read_frame(&mut reader) {
                if sender.send(json).is_err() {
                    break;
                }
            }
        });
        let (outgoing, unsent) = channel::<String>();
        std::thread::spawn(move || {
            let mut writer = BufWriter::new(writer);
            for json in unsent {
                let written = write!(writer, "Content-Length: {}\r\n\r\n{json}", json.len())
                    .and_then(|()| writer.flush());
                if written.is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            outgoing,
            incoming,
            alive: true,
        }
    }

    pub fn send(&mut self, json: String) {
        if self.outgoing.send(json).is_err() {
            self.alive = false;
        }
    }

    pub fn drain(&mut self) -> Vec<String> {
        let mut arrived = Vec::new();
        loop {
            match self.incoming.try_recv() {
                Ok(json) => arrived.push(json),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.alive = false;
                    break;
                }
            }
        }
        arrived
    }
}

impl Drop for Adapter {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn read_frame(reader: &mut impl std::io::BufRead) -> Option<String> {
    let mut length = None;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 {
            return None;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().ok();
            }
        }
    }
    let mut body = vec![0; length?];
    reader.read_exact(&mut body).ok()?;
    String::from_utf8(body).ok()
}

#[cfg(test)]
mod tests {
    use super::{read_frame, Adapter, Server};
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn frames_are_read_by_their_byte_length() {
        let stream = "Content-Length: 12\r\n\r\n{\"seq\":\"é\"}Content-Type: x\r\ncontent-length: 2\r\n\r\n{}";
        let mut reader = std::io::BufReader::new(stream.as_bytes());
        assert_eq!(read_frame(&mut reader).as_deref(), Some("{\"seq\":\"é\"}"));
        assert_eq!(read_frame(&mut reader).as_deref(), Some("{}"));
        assert_eq!(read_frame(&mut reader), None);
    }

    fn large(seq: usize) -> String {
        format!("{{\"seq\":{seq},\"text\":\"{}\"}}", "x".repeat(1 << 20))
    }

    fn notification(text: &str) -> String {
        format!("{{\"jsonrpc\":\"2.0\",\"method\":\"x\",\"params\":{{\"text\":\"{text}\"}}}}")
    }

    #[test]
    fn an_adapter_not_reading_holds_nobody_up_and_gets_every_message_in_order() {
        let (reader, writer) = std::io::pipe().unwrap();
        let mut adapter = Adapter::over(None, std::io::empty(), Box::new(writer));
        let (done, sent) = channel();
        std::thread::spawn(move || {
            for seq in 0..3 {
                adapter.send(large(seq));
            }
            let _ = done.send(adapter);
        });
        let adapter = sent
            .recv_timeout(Duration::from_secs(2))
            .expect("sending waited for the adapter to read");
        assert!(adapter.alive);
        let mut reader = std::io::BufReader::new(reader);
        for seq in 0..3 {
            assert_eq!(read_frame(&mut reader), Some(large(seq)));
        }
    }

    #[test]
    fn an_adapter_that_stops_reading_is_reported_gone() {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        let mut adapter = Adapter::over(None, std::io::empty(), Box::new(writer));
        for _ in 0..200 {
            adapter.send("{}".to_string());
            if !adapter.alive {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("a closed connection was never reported");
    }

    #[test]
    fn a_server_not_reading_holds_nobody_up() {
        let mut server =
            Server::spawn("sleep", &["30".to_string()], &std::env::temp_dir(), None).unwrap();
        let json = notification(&"x".repeat(1 << 20));
        let (done, sent) = channel();
        std::thread::spawn(move || {
            server.send(json);
            let _ = done.send(server);
        });
        let server = sent
            .recv_timeout(Duration::from_secs(2))
            .expect("sending waited for the server to read");
        assert!(server.alive);
    }

    #[test]
    fn a_server_sent_what_the_protocol_cannot_carry_is_reported_gone() {
        let mut server =
            Server::spawn("sleep", &["30".to_string()], &std::env::temp_dir(), None).unwrap();
        for _ in 0..200 {
            server.send("not a message".to_string());
            if !server.alive {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("a writer that stopped was never reported");
    }

    #[test]
    fn a_server_gets_every_message_in_the_order_it_was_sent() {
        let mut server = Server::spawn("cat", &[], &std::env::temp_dir(), None).unwrap();
        for text in ["one", "two", "three"] {
            server.send(notification(text));
        }
        let mut arrived = Vec::new();
        for _ in 0..200 {
            arrived.extend(server.drain());
            if arrived.len() == 3 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let texts: Vec<serde_json::Value> = arrived
            .iter()
            .map(|json| {
                serde_json::from_str::<serde_json::Value>(json).unwrap()["params"]["text"].clone()
            })
            .collect();
        assert_eq!(texts, ["one", "two", "three"]);
    }
}
