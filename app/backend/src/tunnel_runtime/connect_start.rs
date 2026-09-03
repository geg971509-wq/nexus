use super::{connect::PreparedConnect, recovery::fail_connecting};
use crate::{
    core::session::{CoreSession, SESSION},
    session_access::{commit_if_action_current, current_connect_gen, put_session_back},
};

pub(super) enum StartOutcome {
    Started { running: bool, profile_id: i32 },
    Failed { error: String },
    Superseded { profile_id: i32 },
}

pub(super) fn start_core(
    action_gen: u64,
    prepared: &PreparedConnect,
) -> Result<StartOutcome, String> {
    let session_setup = commit_if_action_current(action_gen, || -> Result<CoreSession, String> {
        let mut g = SESSION.lock().map_err(|e| e.to_string())?;
        if let Some(s) = g.as_mut() {
            if s.child_exited() {
                let _ = s.stop_core_process();
                *g = None;
            }
        }
        if g.is_none() {
            let bin = CoreSession::resolve_core_binary();
            if !bin.is_file() {
                return Err(format!("NexusCore not found at {}", bin.display()));
            }
            match CoreSession::start(&bin) {
                Ok(s) => *g = Some(s),
                Err(e) => return Err(e.to_string()),
            }
        }
        let s = g
            .as_mut()
            .ok_or_else(|| "session vanished before start".to_string())?;
        if let Some(bin) = prepared.privileged_core.as_deref() {
            let priv_now = s.is_privileged().unwrap_or(false);
            if !priv_now {
                s.recycle_privileged(bin)?;
            }
        }
        if let Ok((running, _)) = s.query_state() {
            if running {
                let _ = s.stop_rpc();
            }
        }
        g.take()
            .ok_or_else(|| "session vanished before start".to_string())
    });
    let mut session = match session_setup {
        Some(Ok(session)) => session,
        Some(Err(e)) => return Err(fail_connecting(prepared.connect_gen, &prepared.params, e)),
        None => return Err("connect superseded".into()),
    };

    let mut start_err = match session.start_rpc(&prepared.json, prepared.profile_id) {
        Ok(e) => e,
        Err(e) => {
            let _ = put_session_back(session, prepared.connect_gen);
            return Err(fail_connecting(prepared.connect_gen, &prepared.params, e));
        }
    };
    if let Some(ref e) = start_err {
        let el = e.to_ascii_lowercase();
        if el.contains("cache-file") || el.contains("cache.db") || el.contains("timeout") {
            let keep = session.child_pid();
            CoreSession::kill_stray_cores(keep);
            let _ = session.stop_rpc();
            let _ = std::fs::remove_file(CoreSession::cache_db_path());
            start_err = match session.start_rpc(&prepared.json, prepared.profile_id) {
                Ok(e) => e,
                Err(e) => {
                    let _ = put_session_back(session, prepared.connect_gen);
                    return Err(fail_connecting(prepared.connect_gen, &prepared.params, e));
                }
            };
        }
    }
    if let Some(msg) = start_err {
        let _ = put_session_back(session, prepared.connect_gen);
        return Ok(StartOutcome::Failed {
            error: fail_connecting(prepared.connect_gen, &prepared.params, msg),
        });
    }

    let (running, profile_id) = session.query_state().unwrap_or((false, -1));
    if put_session_back(session, prepared.connect_gen) {
        return Ok(StartOutcome::Started {
            running,
            profile_id,
        });
    }
    if current_connect_gen() == prepared.connect_gen {
        let _ = fail_connecting(
            prepared.connect_gen,
            &prepared.params,
            "session not owned after start".into(),
        );
    }
    Ok(StartOutcome::Superseded { profile_id })
}
