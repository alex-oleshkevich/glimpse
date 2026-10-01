use std::path::PathBuf;
use std::time::Instant;

use glimpse_wayland::capture::CaptureError;

use crate::doc::document::Source;
use crate::doc::units::{Logical, Rect};
use crate::ports::Notice;

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    App,
    CurrentScreen,
    Screen(String),
    PickWindow,
    FocusedWindow,
    Window(u64),
    PickRegion,
    Region(Rect<Logical>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    pub headless: bool,
    pub notify: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Countdown {
        until: Instant,
        target: Target,
        face: Face,
    },
    Capturing {
        face: Face,
    },
    Editing {
        saved: Option<PathBuf>,
        copied: bool,
        face: Face,
    },
    Finishing {
        pending: Vec<Effect>,
        done: Vec<Effect>,
        close: bool,
        gone: bool,
        face: Face,
    },
    Done(Outcome),
}

#[derive(Debug)]
pub enum Event {
    Tick(Instant),
    Captured(Result<Source, CaptureError>),
    Finish(FinishKey, Option<PathBuf>),
    Esc,
    Effected(Effect, Result<(), String>),
    OutputGone(String),
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishKey {
    CopyClose,
    SaveAsClose,
    Copy,
    Save,
    OpenInWindow,
    Share,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Capture(Target),
    Encode,
    Write(PathBuf),
    Copy,
    Notify(Notice),
    ShowToast(String),
    Share(PathBuf),
    OpenInWindow,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Saved { path: PathBuf, copied: bool },
    Copied,
    Shared(PathBuf),
    Cancelled,
    TimedOut,
    Failed(String),
}

pub fn start(target: Target, delay: Option<Instant>, face: Face) -> (Phase, Vec<Effect>) {
    match delay {
        Some(until) => (
            Phase::Countdown {
                until,
                target,
                face,
            },
            Vec::new(),
        ),
        None => (Phase::Capturing { face }, vec![Effect::Capture(target)]),
    }
}

pub fn step(phase: Phase, event: Event) -> (Phase, Vec<Effect>) {
    match (phase, event) {
        (
            Phase::Countdown {
                until,
                target,
                face,
            },
            Event::Tick(now),
        ) if now >= until => (Phase::Capturing { face }, vec![Effect::Capture(target)]),
        (Phase::Countdown { .. }, Event::Esc) => done(Outcome::Cancelled),
        (Phase::Countdown { .. }, Event::Timeout) => done(Outcome::TimedOut),
        (Phase::Capturing { face }, Event::Captured(Ok(_))) => (
            Phase::Editing {
                saved: None,
                copied: false,
                face,
            },
            Vec::new(),
        ),
        (Phase::Capturing { .. }, Event::Captured(Err(error))) => {
            done(Outcome::Failed(error.to_string()))
        }
        (Phase::Editing { saved, copied, .. }, Event::Esc) => {
            done(results(saved, copied).unwrap_or(Outcome::Cancelled))
        }
        (Phase::Editing { saved, copied, .. }, Event::Timeout) => {
            done(results(saved, copied).unwrap_or(Outcome::TimedOut))
        }
        (editing @ Phase::Editing { face, .. }, Event::OutputGone(_)) if !face.headless => {
            (editing, vec![Effect::OpenInWindow])
        }
        (
            Phase::Editing {
                saved,
                copied,
                face,
            },
            Event::Finish(key, destination),
        ) => finish(saved, copied, face, key, destination),
        (
            Phase::Finishing {
                pending,
                done,
                close,
                face,
                ..
            },
            Event::OutputGone(_),
        ) if !face.headless => (
            Phase::Finishing {
                pending,
                done,
                close,
                gone: true,
                face,
            },
            Vec::new(),
        ),
        (
            Phase::Finishing {
                pending,
                done,
                close,
                gone,
                face,
            },
            Event::Effected(effect, result),
        ) => {
            let (phase, mut effects) = effected(pending, done, close, gone, face, effect, result);
            if gone && matches!(phase, Phase::Editing { .. }) {
                effects.push(Effect::OpenInWindow);
            }
            (phase, effects)
        }
        (phase, _) => (phase, Vec::new()),
    }
}

fn done(outcome: Outcome) -> (Phase, Vec<Effect>) {
    (Phase::Done(outcome), Vec::new())
}

fn results(saved: Option<PathBuf>, copied: bool) -> Option<Outcome> {
    match (saved, copied) {
        (Some(path), copied) => Some(Outcome::Saved { path, copied }),
        (None, true) => Some(Outcome::Copied),
        (None, false) => None,
    }
}

fn finish(
    saved: Option<PathBuf>,
    copied: bool,
    face: Face,
    key: FinishKey,
    destination: Option<PathBuf>,
) -> (Phase, Vec<Effect>) {
    let mut pending = vec![Effect::Encode];
    let close = matches!(
        key,
        FinishKey::CopyClose | FinishKey::SaveAsClose | FinishKey::Share
    );
    match (key, destination) {
        (FinishKey::OpenInWindow, _) => {
            return (
                Phase::Editing {
                    saved,
                    copied,
                    face,
                },
                vec![Effect::OpenInWindow],
            );
        }
        (FinishKey::CopyClose | FinishKey::Copy, destination) => {
            pending.extend(destination.map(Effect::Write));
            pending.push(Effect::Copy);
        }
        (FinishKey::SaveAsClose | FinishKey::Save, Some(path)) => {
            pending.push(Effect::Write(path));
        }
        (FinishKey::Share, Some(path)) => {
            pending.push(Effect::Write(path.clone()));
            pending.push(Effect::Share(path));
        }
        (_, None) => {
            return failed(face, saved, copied, "there is nowhere to save".to_owned());
        }
    }
    let first = vec![pending[0].clone()];
    let mut done = Vec::new();
    done.extend(saved.map(Effect::Write));
    if copied {
        done.push(Effect::Copy);
    }
    (
        Phase::Finishing {
            pending,
            done,
            close,
            gone: false,
            face,
        },
        first,
    )
}

fn failed(
    face: Face,
    saved: Option<PathBuf>,
    copied: bool,
    reason: String,
) -> (Phase, Vec<Effect>) {
    if face.headless {
        return done(Outcome::Failed(reason));
    }
    (
        Phase::Editing {
            saved,
            copied,
            face,
        },
        vec![Effect::ShowToast(reason)],
    )
}

fn effected(
    mut pending: Vec<Effect>,
    mut done: Vec<Effect>,
    close: bool,
    gone: bool,
    face: Face,
    effect: Effect,
    result: Result<(), String>,
) -> (Phase, Vec<Effect>) {
    let awaited = pending
        .first()
        .is_some_and(|first| std::mem::discriminant(first) == std::mem::discriminant(&effect));
    if !awaited {
        return (
            Phase::Finishing {
                pending,
                done,
                close,
                gone,
                face,
            },
            Vec::new(),
        );
    }
    if let Err(reason) = result {
        let (saved, copied) = so_far(&done);
        return failed(face, saved, copied, reason);
    }
    pending.remove(0);
    if let Effect::Write(actual) = &effect {
        for later in &mut pending {
            if let Effect::Share(path) = later {
                *path = actual.clone();
            }
        }
    }
    done.push(effect);
    if let Some(next) = pending.first().cloned() {
        return (
            Phase::Finishing {
                pending,
                done,
                close,
                gone,
                face,
            },
            vec![next],
        );
    }
    complete(&done, close, face)
}

fn so_far(done: &[Effect]) -> (Option<PathBuf>, bool) {
    let saved = done.iter().rev().find_map(|effect| match effect {
        Effect::Write(path) => Some(path.clone()),
        _ => None,
    });
    (saved, done.contains(&Effect::Copy))
}

fn complete(done: &[Effect], close: bool, face: Face) -> (Phase, Vec<Effect>) {
    let (saved, copied) = so_far(done);
    let shared = done.iter().find_map(|effect| match effect {
        Effect::Share(path) => Some(path.clone()),
        _ => None,
    });
    let outcome = match shared {
        Some(path) => Outcome::Shared(path),
        None => results(saved.clone(), copied).unwrap_or(Outcome::Cancelled),
    };
    let mut effects = Vec::new();
    if face.notify {
        effects.push(Effect::Notify(notice(&outcome)));
    }
    if close {
        return (Phase::Done(outcome), effects);
    }
    let toast = match done.last() {
        Some(Effect::Copy) => "Copied",
        _ => "Saved",
    };
    effects.push(Effect::ShowToast(toast.to_owned()));
    (
        Phase::Editing {
            saved,
            copied,
            face,
        },
        effects,
    )
}

fn notice(outcome: &Outcome) -> Notice {
    let (summary, image) = match outcome {
        Outcome::Saved { path, .. } | Outcome::Shared(path) => ("Screenshot saved", Some(path)),
        _ => ("Screenshot copied", None),
    };
    Notice {
        summary: summary.to_owned(),
        body: image
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_default(),
        image: image.cloned(),
        actions: image.is_some(),
    }
}

#[cfg(test)]
mod tests;
