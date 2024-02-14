//! File modification watcher.

use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use smithay::reexports::calloop::channel::SyncSender;

const DEBOUNCE_DELAY: Duration = Duration::from_millis(100);

/// The fallback for `RecommendedWatcher` polling.
const FALLBACK_POLLING_TIMEOUT: Duration = Duration::from_secs(1);

pub fn watch(path: PathBuf, changed: SyncSender<()>) {
    let Ok(path_metadata) = path.metadata() else {
        warn!("config file {path:?} is not valid");
        return;
    };
    if !path_metadata.file_type().is_file() {
        warn!("config file {path:?} is not a file");
        return;
    }
    let Ok(canonical_path) = path.canonicalize() else {
        warn!("config file {path:?} could not be canonicalized");
        return;
    };
    // When the file is a symlink check both the linked directory and the original one.
    let paths = match canonical_path.symlink_metadata() {
        Ok(symlink_metadata) if symlink_metadata.file_type().is_symlink() => {
            vec![path.clone(), canonical_path]
        }
        _ => vec![canonical_path],
    };

    let mut parents = paths
        .iter()
        .map(|path| {
            let mut path = path.clone();
            path.pop();
            path
        })
        .collect::<Vec<PathBuf>>();
    parents.sort_unstable();
    parents.dedup();

    let (tx, rx) = mpsc::channel();
    let mut watcher = match RecommendedWatcher::new(
        tx,
        Config::default().with_poll_interval(FALLBACK_POLLING_TIMEOUT),
    ) {
        Ok(watcher) => watcher,
        Err(err) => {
            error!("unable to watch config file {path:?}: {err:?}");
            return;
        }
    };

    thread::Builder::new()
        .name(format!("Filesystem Watcher for {}", path.to_string_lossy()))
        .spawn(move || {
            let mut debouncing_deadline: Option<Instant> = None;
            let mut events_received_during_debounce = Vec::new();

            for parent in &parents {
                // Watch the configuration file directory.
                if let Err(err) = watcher.watch(parent, RecursiveMode::NonRecursive) {
                    error!("unable to watch config directory {parent:?}: {err:?}");
                    return;
                }
            }

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
                            .flat_map(|event| event.paths.into_iter())
                            .any(|path| paths.contains(&path))
                        {
                            if let Err(err) = changed.send(()) {
                                warn!("error sending change notification: {err:?}");
                                break;
                            }
                        }
                    }
                    Ok(Err(err)) => {
                        debug!("config watcher errors: {err:?}");
                    }
                    Err(err) => {
                        debug!("config watcher channel dropped unexpectedly: {err:?}");
                        break;
                    }
                }
            }

            debug!("exiting watcher thread for {}", path.to_string_lossy());
        })
        .unwrap();
}
