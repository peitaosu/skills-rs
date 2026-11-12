use std::path::PathBuf;
use std::sync::mpsc;
use std::time::SystemTime;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use skills_rs::SkillEntry;

pub struct SkillWatcher {
    mode: WatchMode,
}

enum WatchMode {
    Snapshot(SnapshotWatcher),
    Notify(NotifyWatcher),
}

struct SnapshotWatcher {
    snapshots: std::collections::HashMap<PathBuf, SystemTime>,
}

struct NotifyWatcher {
    pending: bool,
    receiver: mpsc::Receiver<()>,
    _watcher: RecommendedWatcher,
}

impl SkillWatcher {
    pub fn snapshot(entries: &[SkillEntry]) -> Self {
        Self {
            mode: WatchMode::Snapshot(SnapshotWatcher::new(entries)),
        }
    }

    pub fn live(entries: &[SkillEntry]) -> Result<Self, String> {
        Ok(Self {
            mode: WatchMode::Notify(NotifyWatcher::new(entries)?),
        })
    }

    pub fn changed(&mut self, entries: &[SkillEntry]) -> bool {
        match &mut self.mode {
            WatchMode::Snapshot(watcher) => watcher.changed(entries),
            WatchMode::Notify(watcher) => watcher.poll(),
        }
    }

    pub fn reset(&mut self, entries: &[SkillEntry], live: bool) -> Result<(), String> {
        self.mode = if live {
            WatchMode::Notify(NotifyWatcher::new(entries)?)
        } else {
            WatchMode::Snapshot(SnapshotWatcher::new(entries))
        };
        Ok(())
    }
}

impl SnapshotWatcher {
    fn new(entries: &[SkillEntry]) -> Self {
        let mut snapshots = std::collections::HashMap::new();
        for entry in entries {
            if let Ok(metadata) = fs_metadata(&entry.location) {
                snapshots.insert(entry.location.clone(), metadata);
            }
        }
        Self { snapshots }
    }

    fn changed(&self, entries: &[SkillEntry]) -> bool {
        if entries.len() != self.snapshots.len() {
            return true;
        }

        for entry in entries {
            let Ok(metadata) = fs_metadata(&entry.location) else {
                return true;
            };

            match self.snapshots.get(&entry.location) {
                Some(previous) if previous == &metadata => {}
                _ => return true,
            }
        }

        false
    }
}

impl NotifyWatcher {
    fn new(entries: &[SkillEntry]) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |result: Result<notify::Event, notify::Error>| {
                if let Ok(event) = result
                    && matches!(
                        event.kind,
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                    )
                {
                    let _ = sender.send(());
                }
            },
            notify::Config::default(),
        )
        .map_err(|error| error.to_string())?;

        for entry in entries {
            watcher
                .watch(&entry.skill_dir, RecursiveMode::Recursive)
                .map_err(|error| error.to_string())?;
        }

        Ok(Self {
            pending: false,
            receiver,
            _watcher: watcher,
        })
    }

    fn poll(&mut self) -> bool {
        while self.receiver.try_recv().is_ok() {
            self.pending = true;
        }

        if self.pending {
            self.pending = false;
            return true;
        }

        false
    }
}

fn fs_metadata(path: &std::path::Path) -> Result<SystemTime, ()> {
    path.metadata().map_err(|_| ())?.modified().map_err(|_| ())
}
