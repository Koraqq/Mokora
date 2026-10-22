//! Per-bot `uv sync` mutual exclusion.
//!
//! `uv sync` against an in-flight sync's venv is undefined: lockfile
//! contention, half-written `pyvenv.cfg`, partially-extracted wheels.
//! The IPC `sync_bot_deps` command (user-triggered Reinstall environment)
//! and `BotManager::spawn_runner` (game-start sync) can both fire at the
//! same time, so they share an `Arc<Mutex<HashSet<String>>>` and use this
//! guard to acquire-or-bail on the bot name.

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

/// RAII handle: dropping it removes the bot name from the in-flight set.
pub struct SyncGuard {
    set: Arc<Mutex<HashSet<String>>>,
    name: String,
}

impl SyncGuard {
    /// Try to claim the slot for `name`. `None` if another caller already
    /// holds it. Caller emits its own user-facing error message.
    pub async fn acquire(set: &Arc<Mutex<HashSet<String>>>, name: &str) -> Option<Self> {
        let mut guard = set.lock().await;
        if guard.contains(name) {
            return None;
        }
        guard.insert(name.to_owned());
        Some(Self {
            set: Arc::clone(set),
            name: name.to_owned(),
        })
    }
}

impl Drop for SyncGuard {
    fn drop(&mut self) {
        // Spawn a one-shot task because Drop can't be async. The set is
        // cheap to lock — no contention except other guards' drop paths.
        let set = Arc::clone(&self.set);
        let name = std::mem::take(&mut self.name);
        tokio::spawn(async move {
            set.lock().await.remove(&name);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn second_acquire_returns_none() {
        let set = Arc::new(Mutex::new(HashSet::new()));
        let g1 = SyncGuard::acquire(&set, "mortal").await;
        assert!(g1.is_some());
        let g2 = SyncGuard::acquire(&set, "mortal").await;
        assert!(g2.is_none());
    }

    #[tokio::test]
    async fn distinct_names_are_independent() {
        let set = Arc::new(Mutex::new(HashSet::new()));
        let g1 = SyncGuard::acquire(&set, "mortal").await;
        let g2 = SyncGuard::acquire(&set, "mortal3p").await;
        assert!(g1.is_some());
        assert!(g2.is_some());
    }

    #[tokio::test]
    async fn drop_releases_slot() {
        let set = Arc::new(Mutex::new(HashSet::new()));
        {
            let _g = SyncGuard::acquire(&set, "mortal").await.unwrap();
        }
        // Drop spawns a task; give it a tick to run.
        tokio::task::yield_now().await;
        for _ in 0..10 {
            if !set.lock().await.contains("mortal") {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("name still in set after drop");
    }
}
