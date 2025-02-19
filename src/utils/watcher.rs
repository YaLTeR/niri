//! File modification watcher.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use smithay::reexports::calloop::channel::SyncSender;

pub struct Watcher {
    should_stop: Arc<AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.should_stop.store(true, Ordering::SeqCst);
    }
}

impl Watcher {
    pub fn new<T: Send + 'static>(
        path: PathBuf,
        process: impl FnMut(&Path) -> T + Send + 'static,
        changed: SyncSender<T>,
    ) -> Self {
        Self::with_start_notification(path, process, changed, None)
    }

    pub fn with_start_notification<T: Send + 'static>(
        path: PathBuf,
        mut process: impl FnMut(&Path) -> T + Send + 'static,
        changed: SyncSender<T>,
        started: Option<mpsc::SyncSender<()>>,
    ) -> Self {
        let should_stop = Arc::new(AtomicBool::new(false));

        {
            let should_stop = should_stop.clone();
            thread::Builder::new()
                .name(format!("Filesystem Watcher for {}", path.to_string_lossy()))
                .spawn(move || {
                    // this "should" be as simple as mtime, but it does not quite work in practice;
                    // it doesn't work if the config is a symlink, and its target changes but the
                    // new target and old target have identical mtimes.
                    //
                    // in practice, this does not occur on any systems other than nix.
                    // because, on nix practically everything is a symlink to /nix/store
                    // and due to reproducibility, /nix/store keeps no mtime (= 1970-01-01)
                    // so, symlink targets change frequently when mtime doesn't.
                    let mut last_props = path
                        .canonicalize()
                        .and_then(|canon| Ok((canon.metadata()?.modified()?, canon)))
                        .ok();

                    if let Some(started) = started {
                        let _ = started.send(());
                    }

                    loop {
                        thread::sleep(Duration::from_millis(500));

                        if should_stop.load(Ordering::SeqCst) {
                            break;
                        }

                        if let Ok(new_props) = path
                            .canonicalize()
                            .and_then(|canon| Ok((canon.metadata()?.modified()?, canon)))
                        {
                            if last_props.as_ref() != Some(&new_props) {
                                trace!("file changed: {}", path.to_string_lossy());

                                let rv = process(&path);

                                if let Err(err) = changed.send(rv) {
                                    warn!("error sending change notification: {err:?}");
                                    break;
                                }

                                last_props = Some(new_props);
                            }
                        }
                    }

                    debug!("exiting watcher thread for {}", path.to_string_lossy());
                })
                .unwrap();
        }

        Self { should_stop }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs::File;
    use std::io::Write;
    use std::sync::atomic::AtomicU8;

    use calloop::channel::sync_channel;
    use calloop::EventLoop;
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
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let _watcher =
            Watcher::with_start_notification(config_path.clone(), |_| (), tx, Some(started_tx));
        loop_handle
            .insert_source(rx, |_, _, _| {
                changed.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        started_rx.recv().unwrap();

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
