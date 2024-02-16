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

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs::File;
    use std::io::Write;
    use std::sync::atomic::AtomicU8;

    use calloop::channel::sync_channel;
    use calloop::EventLoop;
    use portable_atomic::Ordering;
    use smithay::reexports::rustix::fs::{futimens, Timestamps};
    use smithay::reexports::rustix::time::Timespec;
    use xshell::{cmd, Shell};

    use super::*;

    fn check(
        setup: impl FnOnce(&Shell) -> Result<(), Box<dyn Error>>,
        change: impl FnOnce(&Shell) -> Result<(), Box<dyn Error>>,
    ) {
        let sh = Shell::new().unwrap();
        let temp_dir = sh.create_temp_dir().unwrap();
        sh.change_dir(temp_dir.path());
        // let dir = sh.create_dir("xshell").unwrap();
        // sh.change_dir(dir);

        let mut config_path = sh.current_dir();
        config_path.push("niri");
        config_path.push("config.kdl");

        setup(&sh).unwrap();

        let changed = AtomicU8::new(0);

        let mut event_loop = EventLoop::try_new().unwrap();
        let loop_handle = event_loop.handle();

        let (tx, rx) = sync_channel(1);
        let _watcher = watch(config_path.clone(), tx);
        loop_handle
            .insert_source(rx, |_, _, _| {
                changed.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();

        // HACK: if we don't sleep, files might have the same mtime.
        thread::sleep(Duration::from_millis(100));

        change(&sh).unwrap();

        event_loop
            .dispatch(Duration::from_millis(750), &mut ())
            .unwrap();

        assert_eq!(changed.load(Ordering::SeqCst), 1);

        // Verify that the watcher didn't break.
        sh.write_file(&config_path, "c").unwrap();

        event_loop
            .dispatch(Duration::from_millis(750), &mut ())
            .unwrap();

        assert_eq!(changed.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn change_file() {
        check(
            |sh| {
                sh.write_file("niri/config.kdl", "a")?;
                Ok(())
            },
            |sh| {
                sh.write_file("niri/config.kdl", "b")?;
                Ok(())
            },
        );
    }

    #[test]
    fn create_file() {
        check(
            |sh| {
                sh.create_dir("niri")?;
                Ok(())
            },
            |sh| {
                sh.write_file("niri/config.kdl", "a")?;
                Ok(())
            },
        );
    }

    #[test]
    fn create_dir_and_file() {
        check(
            |_sh| Ok(()),
            |sh| {
                sh.write_file("niri/config.kdl", "a")?;
                Ok(())
            },
        );
    }

    #[test]
    fn change_linked_file() {
        check(
            |sh| {
                sh.write_file("niri/config2.kdl", "a")?;
                cmd!(sh, "ln -s config2.kdl niri/config.kdl").run()?;
                Ok(())
            },
            |sh| {
                sh.write_file("niri/config2.kdl", "b")?;
                Ok(())
            },
        );
    }

    #[test]
    fn change_file_in_linked_dir() {
        check(
            |sh| {
                sh.write_file("niri2/config.kdl", "a")?;
                cmd!(sh, "ln -s niri2 niri").run()?;
                Ok(())
            },
            |sh| {
                sh.write_file("niri2/config.kdl", "b")?;
                Ok(())
            },
        );
    }

    #[test]
    fn recreate_file() {
        check(
            |sh| {
                sh.write_file("niri/config.kdl", "a")?;
                Ok(())
            },
            |sh| {
                sh.remove_path("niri/config.kdl")?;
                sh.write_file("niri/config.kdl", "b")?;
                Ok(())
            },
        );
    }

    #[test]
    fn recreate_dir() {
        check(
            |sh| {
                sh.write_file("niri/config.kdl", "a")?;
                Ok(())
            },
            |sh| {
                sh.remove_path("niri")?;
                sh.write_file("niri/config.kdl", "b")?;
                Ok(())
            },
        );
    }

    #[test]
    fn swap_dir() {
        check(
            |sh| {
                sh.write_file("niri/config.kdl", "a")?;
                Ok(())
            },
            |sh| {
                sh.write_file("niri2/config.kdl", "b")?;
                sh.remove_path("niri")?;
                cmd!(sh, "mv niri2 niri").run()?;
                Ok(())
            },
        );
    }

    #[test]
    fn swap_just_link() {
        // NixOS setup: link path changes, mtime stays constant.
        check(
            |sh| {
                let mut dir = sh.current_dir();
                dir.push("niri");
                sh.create_dir(&dir)?;

                let mut d2 = dir.clone();
                d2.push("config2.kdl");
                let mut c2 = File::create(d2).unwrap();
                write!(c2, "a")?;
                c2.flush()?;
                futimens(
                    &c2,
                    &Timestamps {
                        last_access: Timespec {
                            tv_sec: 0,
                            tv_nsec: 0,
                        },
                        last_modification: Timespec {
                            tv_sec: 0,
                            tv_nsec: 0,
                        },
                    },
                )?;
                c2.sync_all()?;
                drop(c2);

                let mut d3 = dir.clone();
                d3.push("config3.kdl");
                let mut c3 = File::create(d3).unwrap();
                write!(c3, "b")?;
                c3.flush()?;
                futimens(
                    &c3,
                    &Timestamps {
                        last_access: Timespec {
                            tv_sec: 0,
                            tv_nsec: 0,
                        },
                        last_modification: Timespec {
                            tv_sec: 0,
                            tv_nsec: 0,
                        },
                    },
                )?;
                c3.sync_all()?;
                drop(c3);

                cmd!(sh, "ln -s config2.kdl niri/config.kdl").run()?;
                Ok(())
            },
            |sh| {
                cmd!(sh, "unlink niri/config.kdl").run()?;
                cmd!(sh, "ln -s config3.kdl niri/config.kdl").run()?;
                Ok(())
            },
        );
    }

    #[test]
    fn swap_dir_link() {
        check(
            |sh| {
                sh.write_file("niri2/config.kdl", "a")?;
                cmd!(sh, "ln -s niri2 niri").run()?;
                Ok(())
            },
            |sh| {
                sh.write_file("niri3/config.kdl", "b")?;
                cmd!(sh, "unlink niri").run()?;
                cmd!(sh, "ln -s niri3 niri").run()?;
                Ok(())
            },
        );
    }
}
