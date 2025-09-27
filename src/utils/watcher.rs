//! File modification watcher.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};
use std::{io, thread};

use niri_config::{Config, ConfigPath};
use smithay::reexports::calloop::channel::SyncSender;

use crate::niri::State;

const POLLING_INTERVAL: Duration = Duration::from_millis(500);

pub struct Watcher {
    load_config: mpsc::Sender<()>,
}

struct WatcherInner {
    /// The paths we're watching.
    path: ConfigPath,

    /// Last observed props of the watched file.
    ///
    /// Equality on this means the file did not change.
    ///
    /// We store the absolute path in addition to mtime to account for symlinked configs where the
    /// symlink target may change without mtime. This is common on nix where everything is a
    /// symlink to /nix/store, which keeps no mtime (= 1970-01-01).
    last_props: Option<(SystemTime, PathBuf)>,
}

#[derive(Debug, PartialEq, Eq)]
enum CheckResult {
    Missing,
    Unchanged,
    Changed,
}

impl Watcher {
    pub fn new<T: Send + 'static>(
        path: ConfigPath,
        mut process: impl FnMut(&ConfigPath) -> T + Send + 'static,
        changed: SyncSender<T>,
    ) -> Self {
        let (load_config, load_config_rx) = mpsc::channel();

        thread::Builder::new()
            .name(format!("Filesystem Watcher for {path:?}"))
            .spawn(move || {
                let mut inner = WatcherInner::new(path);

                loop {
                    let mut should_load = match load_config_rx.recv_timeout(POLLING_INTERVAL) {
                        Ok(()) => true,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => false,
                    };

                    match inner.check() {
                        CheckResult::Missing => continue,
                        CheckResult::Unchanged => (),
                        CheckResult::Changed => {
                            trace!("config file changed");
                            should_load = true;
                        }
                    }

                    if should_load {
                        let rv = process(&inner.path);

                        if let Err(err) = changed.send(rv) {
                            warn!("error sending change notification: {err:?}");
                            break;
                        }
                    }
                }

                debug!("exiting watcher thread for {:?}", inner.path);
            })
            .unwrap();

        Self { load_config }
    }

    pub fn load_config(&self) {
        let _ = self.load_config.send(());
    }
}

fn see_path(path: &Path) -> io::Result<(SystemTime, PathBuf)> {
    let canon = path.canonicalize()?;
    let mtime = canon.metadata()?.modified()?;
    Ok((mtime, canon))
}

fn see(config_path: &ConfigPath) -> io::Result<(SystemTime, PathBuf)> {
    match config_path {
        ConfigPath::Explicit(path) => see_path(path),
        ConfigPath::Regular {
            user_path,
            system_path,
        } => see_path(user_path).or_else(|_| see_path(system_path)),
    }
}

impl WatcherInner {
    pub fn new(path: ConfigPath) -> Self {
        let last_props = see(&path).ok();
        Self { path, last_props }
    }

    pub fn check(&mut self) -> CheckResult {
        if let Ok(new_props) = see(&self.path) {
            if self.last_props.as_ref() != Some(&new_props) {
                self.last_props = Some(new_props);
                CheckResult::Changed
            } else {
                CheckResult::Unchanged
            }
        } else {
            CheckResult::Missing
        }
    }
}

pub fn setup(state: &mut State, config_path: &ConfigPath) {
    // Parsing the config actually takes > 20 ms on my beefy machine, so let's do it on the
    // watcher thread.
    let process = |path: &ConfigPath| {
        path.load().config.map_err(|err| {
            warn!("{err:?}");
        })
    };

    let (tx, rx) = calloop::channel::sync_channel(1);
    state
        .niri
        .event_loop
        .insert_source(
            rx,
            |event: calloop::channel::Event<Result<Config, ()>>, _, state| match event {
                calloop::channel::Event::Msg(config) => {
                    let failed = config.is_err();
                    state.reload_config(config);
                    state.ipc_config_loaded(failed);
                }
                calloop::channel::Event::Closed => (),
            },
        )
        .unwrap();

    state.niri.config_file_watcher = Some(Watcher::new(config_path.clone(), process, tx));
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs::{self, File, FileTimes};
    use std::io::Write;

    use xshell::{cmd, Shell, TempDir};

    use super::*;

    type Result<T = (), E = Box<dyn Error>> = std::result::Result<T, E>;

    fn canon(config_path: &ConfigPath) -> &PathBuf {
        match config_path {
            ConfigPath::Explicit(path) => path,
            ConfigPath::Regular {
                user_path,
                system_path,
            } => {
                if user_path.exists() {
                    user_path
                } else {
                    system_path
                }
            }
        }
    }

    enum TestPath<P> {
        Explicit(P),
        Regular { user_path: P, system_path: P },
    }

    impl<P: AsRef<Path>> TestPath<P> {
        fn setup<Discard>(
            self,
            setup: impl FnOnce(&Shell) -> xshell::Result<Discard>,
        ) -> TestSetup {
            self.setup_any(|sh| {
                _ = setup(sh)?;
                Ok(())
            })
        }

        fn without_setup(self) -> TestSetup {
            self.setup_any(|_| Ok(())).assert_initial_not_exists()
        }

        fn setup_any(self, setup: impl FnOnce(&Shell) -> Result) -> TestSetup {
            let sh = Shell::new().unwrap();
            let temp_dir = sh.create_temp_dir().unwrap();
            sh.change_dir(temp_dir.path());

            let dir = sh.current_dir();
            let config_path = match self {
                TestPath::Explicit(path) => ConfigPath::Explicit(dir.join(path)),
                TestPath::Regular {
                    user_path,
                    system_path,
                } => ConfigPath::Regular {
                    user_path: dir.join(user_path),
                    system_path: dir.join(system_path),
                },
            };

            setup(&sh).unwrap();

            TestSetup {
                sh,
                config_path,
                _temp_dir: temp_dir,
            }
        }
    }

    struct TestSetup {
        sh: Shell,
        config_path: ConfigPath,
        _temp_dir: TempDir,
    }

    impl TestSetup {
        fn assert_initial_not_exists(self) -> Self {
            let canon = canon(&self.config_path);
            assert!(!canon.exists(), "initial should not exist");
            self
        }

        fn assert_initial(self, expected: &str) -> Self {
            let canon = canon(&self.config_path);
            assert!(canon.exists(), "initial should exist at {canon:?}");
            let actual = fs::read_to_string(canon).unwrap();
            assert_eq!(actual, expected, "initial file contents do not match");
            self
        }

        fn run(self, body: impl FnOnce(&Shell, &mut TestUtil) -> Result) -> Result {
            let TestSetup {
                sh, config_path, ..
            } = self;

            let mut test = TestUtil {
                watcher: WatcherInner::new(config_path),
            };

            // don't trigger before we start
            test.assert_unchanged();
            // pass_time() inside assert_unchanged() ensures that mtime
            // isn't the same as the initial time

            body(&sh, &mut test)?;

            // nothing should trigger after the test runs
            test.assert_unchanged();

            Ok(())
        }
    }

    struct TestUtil {
        watcher: WatcherInner,
    }

    impl TestUtil {
        // Ensures that mtime is different between writes in the tests.
        fn pass_time(&self) {
            thread::sleep(Duration::from_millis(50));
        }

        fn assert_unchanged(&mut self) {
            let res = self.watcher.check();

            // This may be Missing or Unchanged, both are fine.
            assert_ne!(
                res,
                CheckResult::Changed,
                "watcher should not have noticed any changes"
            );

            self.pass_time();
        }

        fn assert_changed_to(&mut self, expected: &str) {
            let res = self.watcher.check();
            assert_eq!(
                res,
                CheckResult::Changed,
                "watcher should have noticed a change, but it didn't"
            );

            let new_path = canon(&self.watcher.path);
            let actual = fs::read_to_string(new_path).unwrap();
            assert_eq!(actual, expected, "wrong file contents");

            self.pass_time();
        }
    }

    #[test]
    fn change_file() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup(|sh| sh.write_file("niri/config.kdl", "a"))
            .assert_initial("a")
            .run(|sh, test| {
                sh.write_file("niri/config.kdl", "b")?;
                test.assert_changed_to("b");

                Ok(())
            })
    }

    #[test]
    fn overwrite_but_dont_change_file() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup(|sh| sh.write_file("niri/config.kdl", "a"))
            .assert_initial("a")
            .run(|sh, test| {
                sh.write_file("niri/config.kdl", "a")?;
                test.assert_changed_to("a");

                Ok(())
            })
    }

    #[test]
    fn touch_file() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup(|sh| sh.write_file("niri/config.kdl", "a"))
            .assert_initial("a")
            .run(|sh, test| {
                cmd!(sh, "touch niri/config.kdl").run()?;
                test.assert_changed_to("a");

                Ok(())
            })
    }

    #[test]
    fn create_file() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup(|sh| sh.create_dir("niri"))
            .assert_initial_not_exists()
            .run(|sh, test| {
                sh.write_file("niri/config.kdl", "a")?;
                test.assert_changed_to("a");

                Ok(())
            })
    }

    #[test]
    fn create_dir_and_file() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .without_setup()
            .run(|sh, test| {
                sh.write_file("niri/config.kdl", "a")?;
                test.assert_changed_to("a");

                Ok(())
            })
    }

    #[test]
    fn change_linked_file() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup(|sh| {
                sh.write_file("niri/config2.kdl", "a")?;
                cmd!(sh, "ln -sf config2.kdl niri/config.kdl").run()
            })
            .assert_initial("a")
            .run(|sh, test| {
                sh.write_file("niri/config2.kdl", "b")?;
                test.assert_changed_to("b");

                Ok(())
            })
    }

    #[test]
    fn change_file_in_linked_dir() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup(|sh| {
                sh.write_file("niri2/config.kdl", "a")?;
                cmd!(sh, "ln -s niri2 niri").run()
            })
            .assert_initial("a")
            .run(|sh, test| {
                sh.write_file("niri2/config.kdl", "b")?;
                test.assert_changed_to("b");

                Ok(())
            })
    }

    #[test]
    fn remove_file() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup(|sh| sh.write_file("niri/config.kdl", "a"))
            .assert_initial("a")
            .run(|sh, test| {
                sh.remove_path("niri/config.kdl")?;
                test.assert_unchanged();

                Ok(())
            })
    }

    #[test]
    fn remove_dir() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup(|sh| sh.write_file("niri/config.kdl", "a"))
            .assert_initial("a")
            .run(|sh, test| {
                sh.remove_path("niri")?;
                test.assert_unchanged();

                Ok(())
            })
    }

    #[test]
    fn recreate_file() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup(|sh| sh.write_file("niri/config.kdl", "a"))
            .assert_initial("a")
            .run(|sh, test| {
                sh.remove_path("niri/config.kdl")?;
                sh.write_file("niri/config.kdl", "b")?;
                test.assert_changed_to("b");

                Ok(())
            })
    }

    #[test]
    fn recreate_dir() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup(|sh| {
                sh.write_file("niri/config.kdl", "a")?;
                Ok(())
            })
            .assert_initial("a")
            .run(|sh, test| {
                sh.remove_path("niri")?;
                sh.write_file("niri/config.kdl", "b")?;
                test.assert_changed_to("b");

                Ok(())
            })
    }

    #[test]
    fn swap_dir() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup(|sh| sh.write_file("niri/config.kdl", "a"))
            .assert_initial("a")
            .run(|sh, test| {
                sh.write_file("niri2/config.kdl", "b")?;
                sh.remove_path("niri")?;
                cmd!(sh, "mv niri2 niri").run()?;
                test.assert_changed_to("b");

                Ok(())
            })
    }

    #[test]
    fn swap_dir_link() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup(|sh| {
                sh.write_file("niri2/config.kdl", "a")?;
                cmd!(sh, "ln -s niri2 niri").run()
            })
            .assert_initial("a")
            .run(|sh, test| {
                sh.write_file("niri3/config.kdl", "b")?;
                sh.remove_path("niri")?;
                cmd!(sh, "ln -s niri3 niri").run()?;
                test.assert_changed_to("b");

                Ok(())
            })
    }

    // Important: On systems like NixOS, mtime is not kept for config files.
    // So, this is testing that the watcher handles that correctly.
    fn create_epoch(path: impl AsRef<Path>, content: &str) -> Result {
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())?;
        file.set_times(
            FileTimes::new()
                .set_accessed(SystemTime::UNIX_EPOCH)
                .set_modified(SystemTime::UNIX_EPOCH),
        )?;
        file.sync_all()?;
        Ok(())
    }

    #[test]
    fn swap_just_link() -> Result {
        TestPath::Explicit("niri/config.kdl")
            .setup_any(|sh| {
                let dir = sh.current_dir().join("niri");

                sh.create_dir(&dir)?;

                create_epoch(dir.join("config2.kdl"), "a")?;
                create_epoch(dir.join("config3.kdl"), "b")?;

                cmd!(sh, "ln -s config2.kdl niri/config.kdl").run()?;

                Ok(())
            })
            .assert_initial("a")
            .run(|sh, test| {
                cmd!(sh, "ln -sf config3.kdl niri/config.kdl").run()?;
                test.assert_changed_to("b");

                Ok(())
            })
    }

    #[test]
    fn swap_many_regular() -> Result {
        TestPath::Regular {
            user_path: "user-niri/config.kdl",
            system_path: "system-niri/config.kdl",
        }
        .setup(|sh| sh.write_file("system-niri/config.kdl", "system config"))
        .assert_initial("system config")
        .run(|sh, test| {
            sh.write_file("user-niri/config.kdl", "user config")?;
            test.assert_changed_to("user config");

            cmd!(sh, "touch system-niri/config.kdl").run()?;
            test.assert_unchanged();

            sh.remove_path("system-niri")?;
            test.assert_unchanged();

            sh.write_file("system-niri/config.kdl", "new system config")?;
            test.assert_unchanged();

            sh.remove_path("user-niri")?;
            test.assert_changed_to("new system config");

            sh.write_file("system-niri/config.kdl", "updated system config")?;
            test.assert_changed_to("updated system config");

            sh.write_file("user-niri/config.kdl", "new user config")?;
            test.assert_changed_to("new user config");

            Ok(())
        })
    }

    #[test]
    fn swap_many_links_regular_like_nix() -> Result {
        TestPath::Regular {
            user_path: "user-niri/config.kdl",
            system_path: "system-niri/config.kdl",
        }
        .setup_any(|sh| {
            let store = sh.current_dir().join("store");

            sh.create_dir(&store)?;

            create_epoch(store.join("gen1"), "gen 1")?;
            create_epoch(store.join("gen2"), "gen 2")?;
            create_epoch(store.join("gen3"), "gen 3")?;

            sh.create_dir("user-niri")?;
            sh.create_dir("system-niri")?;

            Ok(())
        })
        .assert_initial_not_exists()
        .run(|sh, test| {
            let store = sh.current_dir().join("store");
            test.assert_unchanged();

            cmd!(sh, "ln -s {store}/gen1 user-niri/config.kdl").run()?;
            test.assert_changed_to("gen 1");

            cmd!(sh, "ln -s {store}/gen2 system-niri/config.kdl").run()?;
            test.assert_unchanged();

            cmd!(sh, "unlink user-niri/config.kdl").run()?;
            test.assert_changed_to("gen 2");

            cmd!(sh, "ln -s {store}/gen3 user-niri/config.kdl").run()?;
            test.assert_changed_to("gen 3");

            cmd!(sh, "ln -sf {store}/gen1 system-niri/config.kdl").run()?;
            test.assert_unchanged();

            cmd!(sh, "unlink system-niri/config.kdl").run()?;
            test.assert_unchanged();

            cmd!(sh, "ln -s {store}/gen1 system-niri/config.kdl").run()?;
            test.assert_unchanged();

            cmd!(sh, "unlink user-niri/config.kdl").run()?;
            test.assert_changed_to("gen 1");

            Ok(())
        })
    }
}
