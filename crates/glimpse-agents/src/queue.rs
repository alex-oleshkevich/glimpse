use std::collections::VecDeque;

use crate::ask::{Ask, Reply, Source};

#[derive(Default)]
pub struct Queue {
    entries: VecDeque<Ask>,
    locked: bool,
}

impl Queue {
    pub fn push(&mut self, ask: Ask) {
        if self.locked && ask.source != Source::Polkit {
            refuse(ask);
            return;
        }
        match self.entries.iter_mut().find(|queued| {
            queued.source == ask.source
                && queued.key == ask.key
                && matches!(queued.reply, Reply::Shown)
        }) {
            Some(queued) => *queued = ask,
            None => self.entries.push_back(ask),
        }
    }

    pub fn withdraw(&mut self, source: Source, key: Option<&str>) {
        self.entries
            .retain(|queued| queued.source != source || key.is_some_and(|key| queued.key != key));
    }

    pub fn lock(&mut self, locked: bool) {
        self.locked = locked;
        if !locked {
            return;
        }
        for ask in std::mem::take(&mut self.entries) {
            match ask.source {
                Source::Polkit => self.entries.push_back(ask),
                Source::Network | Source::Bluetooth => refuse(ask),
            }
        }
    }

    pub fn head(&self) -> Option<&Ask> {
        self.entries.front()
    }

    pub fn shown(&self) -> Option<&Ask> {
        self.head().filter(|_| !self.locked)
    }

    pub fn take_head(&mut self) -> Option<Ask> {
        self.entries.pop_front()
    }
}

fn refuse(ask: Ask) {
    let _ = match ask.reply {
        Reply::Secret(reply) => reply.send(crate::network::Answer::Refused).is_ok(),
        Reply::Pairing(reply) => reply.send(crate::bluetooth::Answer::Deny).is_ok(),
        Reply::Polkit(_) | Reply::Shown => false,
    };
}

#[cfg(test)]
mod tests;
