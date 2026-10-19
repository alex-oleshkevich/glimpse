use crate::context::App;
use crate::protocol::{Check, Fail, In, MAX_LINE};
use crate::session::Session;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, Sender, channel};

pub(crate) fn fail_code(fail: &Fail) -> u8 {
    match fail {
        Fail::Limit(_) | Fail::Render(_) => 70,
        Fail::Unavailable(_) => 69,
        Fail::Io(_) => 1,
    }
}

pub fn run<A: App>(app: A) -> ExitCode {
    let socket = std::env::var("GLIMPSE_APPLET_SOCKET").unwrap_or_default();
    match run_inner(app, &socket) {
        Ok(()) => ExitCode::from(0),
        Err(fail) => {
            eprintln!("{fail}");
            ExitCode::from(fail_code(&fail))
        }
    }
}

pub(crate) fn run_inner<A: App>(app: A, socket: &str) -> Check<()> {
    let (tx, rx): (Sender<In>, Receiver<In>) = channel();
    if socket.is_empty() {
        return Err(Fail::Io(
            "GLIMPSE_APPLET_SOCKET is not set: run the applet under the panel".to_owned(),
        ));
    }

    let mut writer: Box<dyn Write> = match UnixStream::connect(socket) {
        Ok(stream) => {
            let reader_stream = stream
                .try_clone()
                .map_err(|error| Fail::Io(error.to_string()))?;
            spawn_reader(Box::new(reader_stream), tx.clone());
            Box::new(stream)
        }
        Err(error) => match error.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
                return Err(Fail::Unavailable(format!(
                    "GLIMPSE_APPLET_SOCKET at {socket} is not reachable: run the applet under the panel"
                )));
            }
            _ => return Err(Fail::Io(error.to_string())),
        },
    };

    let mut session = Session::new(app, tx);

    write_line(&mut writer, "{\"t\":\"hello\",\"v\":1}")?;

    let mut pending: std::collections::VecDeque<In> = std::collections::VecDeque::new();
    loop {
        if pending.is_empty() {
            match rx.recv() {
                Ok(value) => pending.push_back(value),
                Err(_) => return Ok(()),
            }
        }

        while let Ok(value) = rx.try_recv() {
            pending.push_back(value);
        }

        let mut eof = false;
        while let Some(input) = pending.pop_front() {
            match input {
                In::Eof => eof = true,
                In::Line(line) => {
                    if line.trim().is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Value>(&line) {
                        Ok(msg) if msg["t"].is_string() => {
                            let outcome = catch_unwind(AssertUnwindSafe(|| session.handle(&msg)));
                            if outcome.is_err() {
                                std::process::exit(1);
                            }
                        }
                        Ok(_) => {}
                        Err(error) => eprintln!("{error}"),
                    }
                }
                In::Send(line) => write_line(&mut writer, &line)?,
                In::Wake => session.dirty = true,
            }
        }

        session.flush()?;
        while let Ok(value) = rx.try_recv() {
            pending.push_back(value);
        }

        let mut rest = std::collections::VecDeque::new();
        while let Some(input) = pending.pop_front() {
            match input {
                In::Send(line) => write_line(&mut writer, &line)?,
                In::Eof => eof = true,
                other => rest.push_back(other),
            }
        }

        pending = rest;
        if eof {
            return Ok(());
        }
    }
}

fn spawn_reader(reader: Box<dyn Read + Send>, tx: Sender<In>) {
    std::thread::spawn(move || {
        let mut lines = BufReader::new(reader).lines();
        loop {
            match lines.next() {
                Some(Ok(line)) => {
                    if tx.send(In::Line(line)).is_err() {
                        return;
                    }
                }
                _ => {
                    let _ = tx.send(In::Eof);
                    return;
                }
            }
        }
    });
}

fn write_line(writer: &mut dyn Write, line: &str) -> Check<()> {
    if line.len() > MAX_LINE {
        return Err(Fail::Limit("outgoing line exceeds 1 MiB".to_owned()));
    }

    let _ = writer.write_all(line.as_bytes());
    let _ = writer.write_all(b"\n");
    let _ = writer.flush();
    Ok(())
}
