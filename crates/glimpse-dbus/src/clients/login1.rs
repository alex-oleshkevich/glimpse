use zbus::zvariant::OwnedFd;
use zbus::zvariant::OwnedObjectPath;

pub type Login1SessionEntry = (String, u32, String, String, OwnedObjectPath);

/// Returned by Manager.ListInhibitors — (what, who, why, mode, uid, pid)
pub type Login1InhibitorEntry = (String, String, String, String, u32, u32);

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
pub trait Login1Manager {
    fn get_session(&self, session_id: &str) -> zbus::Result<OwnedObjectPath>;
    #[zbus(name = "GetSessionByPID")]
    fn get_session_by_pid(&self, pid: u32) -> zbus::Result<OwnedObjectPath>;
    fn list_sessions(&self) -> zbus::Result<Vec<Login1SessionEntry>>;
    // Returns one of: "yes" | "no" | "na" | "challenge"
    //               | "inhibited" | "inhibitor-blocked" | "challenge-inhibitor-blocked".
    fn can_suspend(&self) -> zbus::Result<String>;
    // Returns one of: "yes" | "no" | "na" | "challenge"
    //               | "inhibited" | "inhibitor-blocked" | "challenge-inhibitor-blocked".
    fn can_hibernate(&self) -> zbus::Result<String>;
    // Returns one of: "yes" | "no" | "na" | "challenge"
    //               | "inhibited" | "inhibitor-blocked" | "challenge-inhibitor-blocked".
    fn can_reboot(&self) -> zbus::Result<String>;
    // Returns one of: "yes" | "no" | "na" | "challenge"
    //               | "inhibited" | "inhibitor-blocked" | "challenge-inhibitor-blocked".
    fn can_power_off(&self) -> zbus::Result<String>;
    fn suspend(&self, interactive: bool) -> zbus::Result<()>;
    fn hibernate(&self, interactive: bool) -> zbus::Result<()>;
    fn reboot(&self, interactive: bool) -> zbus::Result<()>;
    fn power_off(&self, interactive: bool) -> zbus::Result<()>;
    fn terminate_session(&self, session_id: &str) -> zbus::Result<()>;
    fn lock_session(&self, session_id: &str) -> zbus::Result<()>;

    fn inhibit(&self, what: &str, who: &str, why: &str, mode: &str) -> zbus::Result<OwnedFd>;

    fn list_inhibitors(&self) -> zbus::Result<Vec<Login1InhibitorEntry>>;

    #[zbus(property, name = "BlockInhibited")]
    fn block_inhibited(&self) -> zbus::Result<String>;

    #[zbus(property, name = "DelayInhibited")]
    fn delay_inhibited(&self) -> zbus::Result<String>;

    #[zbus(signal, name = "SessionNew")]
    fn session_new(&self, session_id: &str, path: OwnedObjectPath) -> zbus::Result<()>;

    #[zbus(signal, name = "SessionRemoved")]
    fn session_removed(&self, session_id: &str, path: OwnedObjectPath) -> zbus::Result<()>;

    #[zbus(signal, name = "PrepareForSleep")]
    fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1"
)]
pub trait Login1Session {
    fn set_locked_hint(&self, locked: bool) -> zbus::Result<()>;
    fn set_brightness(&self, subsystem: &str, name: &str, brightness: u32) -> zbus::Result<()>;

    #[zbus(property, name = "LockedHint")]
    fn locked_hint(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn active(&self) -> zbus::Result<bool>;
    #[zbus(property, name = "Class")]
    fn class(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property, name = "Id")]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn seat(&self) -> zbus::Result<(String, OwnedObjectPath)>;
    #[zbus(property, name = "Type")]
    fn kind(&self) -> zbus::Result<String>;
    /// `(uid, user_object_path)` tuple identifying the session's owner.
    #[zbus(property, name = "User")]
    fn user(&self) -> zbus::Result<(u32, OwnedObjectPath)>;
    #[zbus(property, name = "Timestamp")]
    fn timestamp(&self) -> zbus::Result<u64>;

    #[zbus(signal)]
    fn lock(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn unlock(&self) -> zbus::Result<()>;
}

#[derive(Clone, Debug)]
pub struct SessionCandidate {
    pub id: String,
    pub uid: u32,
    pub seat: String,
    pub path: OwnedObjectPath,
    pub active: bool,
    pub class: Option<String>,
    pub kind: Option<String>,
}

pub fn select_session_candidate(
    candidates: &[SessionCandidate],
    uid: u32,
) -> Option<&SessionCandidate> {
    let candidates = candidates
        .iter()
        .filter(|candidate| candidate.uid == uid)
        .collect::<Vec<_>>();

    select_best_session(candidates.iter().copied().filter(|candidate| {
        candidate.class.as_deref() == Some("user") && candidate.kind.as_deref() == Some("wayland")
    }))
    .or_else(|| {
        select_best_session(candidates.iter().copied().filter(|candidate| {
            candidate.class.as_deref() == Some("user") && candidate.kind.as_deref() == Some("x11")
        }))
    })
    .or_else(|| {
        select_best_session(
            candidates
                .iter()
                .copied()
                .filter(|candidate| candidate.class.as_deref() == Some("user")),
        )
    })
    .or_else(|| select_best_session(candidates.iter().copied()))
}

pub async fn session_path(bus: &zbus::Connection) -> Result<OwnedObjectPath, String> {
    let manager = Login1ManagerProxy::new(bus)
        .await
        .map_err(|error| error.to_string())?;
    let uid = current_uid();
    let mut candidates = Vec::new();
    for (id, candidate_uid, _, seat, path) in manager
        .list_sessions()
        .await
        .map_err(|error| error.to_string())?
    {
        if candidate_uid != uid {
            continue;
        }
        let candidate = async {
            let session = Login1SessionProxy::builder(bus)
                .path(path.clone())
                .map_err(|error| error.to_string())?
                .build()
                .await
                .map_err(|error| error.to_string())?;
            let (active, class, kind) =
                tokio::try_join!(session.active(), session.class(), session.kind())
                    .map_err(|error| error.to_string())?;
            Ok::<_, String>(SessionCandidate {
                id,
                uid: candidate_uid,
                seat,
                path,
                active,
                class: Some(class),
                kind: Some(kind),
            })
        }
        .await;
        match candidate {
            Ok(candidate) => candidates.push(candidate),
            Err(error) => tracing::debug!(%error, "skipping a partial login session"),
        }
    }
    select_session_candidate(&candidates, uid)
        .map(|candidate| candidate.path.clone())
        .ok_or_else(|| "the current user has no login session".to_owned())
}

fn select_best_session<'a>(
    candidates: impl Iterator<Item = &'a SessionCandidate>,
) -> Option<&'a SessionCandidate> {
    candidates.max_by_key(|candidate| session_candidate_score(candidate))
}

fn session_candidate_score(candidate: &SessionCandidate) -> u32 {
    let mut score = 0;
    if candidate.active {
        score += 100;
    }
    if candidate.class.as_deref() == Some("user") {
        score += 40;
    }
    if candidate.kind.as_deref() == Some("wayland") {
        score += 30;
    }
    if candidate.kind.as_deref() == Some("x11") {
        score += 20;
    }
    if !candidate.seat.is_empty() {
        score += 10;
    }
    score
}

pub fn current_uid() -> u32 {
    rustix::process::getuid().as_raw()
}

#[cfg(test)]
mod tests;
