//! File modification watcher.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use smithay::reexports::calloop::channel::SyncSender;

const DEBOUNCE_DELAY: Duration = Duration::from_millis(100);

/// The fallback for `RecommendedWatcher` polling.
const FALLBACK_POLLING_TIMEOUT: Duration = Duration::from_secs(1);

pub fn watch(path: PathBuf, changed: SyncSender<()>) {
    let (tx, rx) = mpsc::channel();
    let mut watcher = match RecommendedWatcher::new(
        tx,
        Config::default().with_poll_interval(FALLBACK_POLLING_TIMEOUT),
    ) {
        Ok(watcher) => watcher,
        Err(err) => {
            error!("Unable to watch config file {path:?}: {err}");
            return;
        }
    };

    thread::Builder::new()
        .name(format!("Filesystem Watcher for {}", path.to_string_lossy()))
        .spawn(move || {
            // Watch the configuration file directory.
            let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
            if let Err(err) = watcher.watch(&parent, RecursiveMode::NonRecursive) {
                error!("Unable to watch config directory {parent:?}: {err}");
            }

            let mut debouncing_deadline: Option<Instant> = None;
            let mut events_received_during_debounce = Vec::new();

            loop {
                let event = match debouncing_deadline.as_ref() {
                    Some(debouncing_deadline) => rx.recv_timeout(
                        debouncing_deadline.saturating_duration_since(Instant::now()),
                    ),
                    None => {
                        let event = rx.recv().map_err(Into::into);

                        debouncing_deadline = Some(Instant::now() + DEBOUNCE_DELAY);

                        event
                    }
                };

                match event {
                    Ok(Ok(event)) => match event.kind {
                        EventKind::Any
                        | EventKind::Create(_)
                        | EventKind::Modify(_)
                        | EventKind::Other => {
                            events_received_during_debounce.push(event);
                        }
                        _ => (),
                    },
                    Err(RecvTimeoutError::Timeout) => {
                        debouncing_deadline = None;

                        if events_received_during_debounce
                            .drain(..)
                            .any(|event| event.paths.contains(&path))
                        {
                            if let Err(err) = changed.send(()) {
                                warn!("error sending change notification: {err:?}");
                                break;
                            }
                        }
                    }
                    Ok(Err(err)) => {
                        debug!("Config watcher errors: {err:?}");
                    }
                    Err(err) => {
                        debug!("Config watcher channel dropped unexpectedly: {err}");
                        break;
                    }
                }
            }

            debug!("Exiting watcher thread for {}", path.to_string_lossy());
        })
        .unwrap();
}
