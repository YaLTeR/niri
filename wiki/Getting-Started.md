The easiest way to get niri is to install one of the distribution packages.
Here are some of them: [Fedora COPR](https://copr.fedorainfracloud.org/coprs/yalter/niri/) and [nightly COPR](https://copr.fedorainfracloud.org/coprs/yalter/niri-git/) (which I maintain myself), [NixOS Flake](https://github.com/sodiboo/niri-flake), and some more from repology below.
See the [Building](#building) section if you'd like to compile niri yourself.

[![Packaging status](https://repology.org/badge/vertical-allrepos/niri.svg)](https://repology.org/project/niri/versions)

After installing, start niri from your display manager like GDM.
Press <kbd>Super</kbd><kbd>T</kbd> to run a terminal ([Alacritty]) and <kbd>Super</kbd><kbd>D</kbd> to run an application launcher ([fuzzel]).
To exit niri, press <kbd>Super</kbd><kbd>Shift</kbd><kbd>E</kbd>.

If you're not using a display manager, you should run `niri-session` (systemd/dinit) or `niri --session` (others) from a TTY.
The `--session` flag will make niri import its environment variables globally into the system manager and D-Bus, and start its D-Bus services.

You can also run `niri` inside an existing desktop session.
Then it will open as a window, where you can give it a try.
Note that this windowed mode is mainly meant for development, so it is a bit buggy (in particular, there are issues with hotkeys).

Next, see the [list of important software](./Important-Software.md) required for normal desktop use, like a notification daemon and portals.
Also, check the [configuration overview](./Configuration:-Overview.md) page to get started configuring niri.
There you can find links to other pages containing thorough documentation and examples for all options.
Finally, the [Xwayland](./Xwayland.md) page explains how to run X11 applications on niri.

### NVIDIA

NVIDIA GPUs tend to have problems running niri (for example, the screen remains black upon starting from a TTY).
Sometimes, the problems can be fixed.
You can try the following:

1. Update NVIDIA drivers. You need a GPU and drivers recent enough to support GBM.
2. Make sure kernel modesetting is enabled. This usually involves adding `nvidia-drm.modeset=1` to the kernel command line. Find and follow a guide for your distribution. Guides from other Wayland compositors can help.

If niri runs but the screen flickers, try adding this into your niri config:

```
debug {
    wait-for-frame-completion-before-queueing
}
```

### Asahi, ARM, and other kmsro devices

On some of these systems, niri fails to correctly detect the primary render device.
If you're getting a black screen when starting niri on a TTY, you can try to set the device manually.

First, find which devices you have:

```
$ ls -l /dev/dri/
drwxr-xr-x@       - root 14 мая 07:07 by-path
crw-rw----@   226,0 root 14 мая 07:07 card0
crw-rw----@   226,1 root 14 мая 07:07 card1
crw-rw-rw-@ 226,128 root 14 мая 07:07 renderD128
crw-rw-rw-@ 226,129 root 14 мая 07:07 renderD129
```

You will likely have one `render` device and two `card` devices.

Open the niri config file at `~/.config/niri/config.kdl` and put your `render` device path like this:

```
debug {
    render-drm-device "/dev/dri/renderD128"
}
```

Save, then try to start niri again.
If you still get a black screen, try using each of the `card` devices.

### Nix/NixOS

There's a common problem of mesa drivers going out of sync with niri, so make sure your system mesa version matches the niri mesa version.

Also, on Intel graphics, you may need a workaround described [here](https://nixos.wiki/wiki/Intel_Graphics).

### Virtual Machines

To run niri in a VM, make sure to enable 3D acceleration.

## Default Hotkeys

When running on a TTY, the Mod key is <kbd>Super</kbd>.
When running in a window, the Mod key is <kbd>Alt</kbd>.

The general system is: if a hotkey switches somewhere, then adding <kbd>Ctrl</kbd> will move the focused window or column there.

| Hotkey | Description |
| ------ | ----------- |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>/</kbd> | Show a list of important niri hotkeys |
| <kbd>Mod</kbd><kbd>T</kbd> | Spawn `alacritty` (terminal) |
| <kbd>Mod</kbd><kbd>D</kbd> | Spawn `fuzzel` (application launcher) |
| <kbd>Super</kbd><kbd>Alt</kbd><kbd>L</kbd> | Spawn `swaylock` (screen locker) |
| <kbd>Mod</kbd><kbd>Q</kbd> | Close the focused window |
| <kbd>Mod</kbd><kbd>H</kbd> or <kbd>Mod</kbd><kbd>←</kbd> | Focus the column to the left |
| <kbd>Mod</kbd><kbd>L</kbd> or <kbd>Mod</kbd><kbd>→</kbd> | Focus the column to the right |
| <kbd>Mod</kbd><kbd>J</kbd> or <kbd>Mod</kbd><kbd>↓</kbd> | Focus the window below in a column |
| <kbd>Mod</kbd><kbd>K</kbd> or <kbd>Mod</kbd><kbd>↑</kbd> | Focus the window above in a column |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>H</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>←</kbd> | Move the focused column to the left |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>L</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>→</kbd> | Move the focused column to the right |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>J</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>↓</kbd> | Move the focused window below in a column |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>K</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>↑</kbd> | Move the focused window above in a column |
| <kbd>Mod</kbd><kbd>Home</kbd> and <kbd>Mod</kbd><kbd>End</kbd> | Focus the first or the last column |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>Home</kbd> and <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>End</kbd> | Move the focused column to the very start or to the very end |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>H</kbd><kbd>J</kbd><kbd>K</kbd><kbd>L</kbd> or <kbd>Mod</kbd><kbd>Shift</kbd><kbd>←</kbd><kbd>↓</kbd><kbd>↑</kbd><kbd>→</kbd> | Focus the monitor to the side |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>H</kbd><kbd>J</kbd><kbd>K</kbd><kbd>L</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>←</kbd><kbd>↓</kbd><kbd>↑</kbd><kbd>→</kbd> | Move the focused column to the monitor to the side |
| <kbd>Mod</kbd><kbd>U</kbd> or <kbd>Mod</kbd><kbd>PageDown</kbd> | Switch to the workspace below |
| <kbd>Mod</kbd><kbd>I</kbd> or <kbd>Mod</kbd><kbd>PageUp</kbd> | Switch to the workspace above |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>U</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>PageDown</kbd> | Move the focused column to the workspace below |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>I</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>PageUp</kbd> | Move the focused column to the workspace above |
| <kbd>Mod</kbd><kbd>1</kbd>–<kbd>9</kbd> | Switch to a workspace by index |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>1</kbd>–<kbd>9</kbd> | Move the focused column to a workspace by index |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>U</kbd> or <kbd>Mod</kbd><kbd>Shift</kbd><kbd>PageDown</kbd> | Move the focused workspace down |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>I</kbd> or <kbd>Mod</kbd><kbd>Shift</kbd><kbd>PageUp</kbd> | Move the focused workspace up |
| <kbd>Mod</kbd><kbd>,</kbd> | Consume the window to the right into the focused column |
| <kbd>Mod</kbd><kbd>.</kbd> | Expel the focused window into its own column |
| <kbd>Mod</kbd><kbd>[</kbd> | Consume or expel the focused window to the left |
| <kbd>Mod</kbd><kbd>]</kbd> | Consume or expel the focused window to the right |
| <kbd>Mod</kbd><kbd>R</kbd> | Toggle between preset column widths |
| <kbd>Mod</kbd><kbd>F</kbd> | Maximize column |
| <kbd>Mod</kbd><kbd>C</kbd> | Center column within view |
| <kbd>Mod</kbd><kbd>-</kbd> | Decrease column width by 10% |
| <kbd>Mod</kbd><kbd>=</kbd> | Increase column width by 10% |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>-</kbd> | Decrease window height by 10% |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>=</kbd> | Increase window height by 10% |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>R</kbd> | Reset window height back to automatic |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>F</kbd> | Toggle full-screen on the focused window |
| <kbd>PrtSc</kbd> | Take an area screenshot. Select the area to screenshot with mouse, then press Space to save the screenshot, or Escape to cancel |
| <kbd>Alt</kbd><kbd>PrtSc</kbd> | Take a screenshot of the focused window to clipboard and to `~/Pictures/Screenshots/` |
| <kbd>Ctrl</kbd><kbd>PrtSc</kbd> | Take a screenshot of the focused monitor to clipboard and to `~/Pictures/Screenshots/` |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>E</kbd> | Exit niri |

## Building

First, install the dependencies for your distribution.

- Ubuntu 23.10:

    ```sh
    sudo apt-get install -y gcc clang libudev-dev libgbm-dev libxkbcommon-dev libegl1-mesa-dev libwayland-dev libinput-dev libdbus-1-dev libsystemd-dev libseat-dev libpipewire-0.3-dev libpango1.0-dev libdisplay-info-dev
    ```

- Fedora:

    ```sh
    sudo dnf install gcc libudev-devel libgbm-devel libxkbcommon-devel wayland-devel libinput-devel dbus-devel systemd-devel libseat-devel pipewire-devel pango-devel cairo-gobject-devel clang libdisplay-info-devel
    ```

Next, get latest stable Rust: https://rustup.rs/

Then, build niri with `cargo build --release`.

Check Cargo.toml for a list of build features.
For example, you can replace systemd integration with dinit integration using `cargo build --release --no-default-features --features dinit,dbus,xdp-gnome-screencast`.

> [!WARNING]
> Do NOT build with `--all-features`!
>
> Some features are meant only for development use.
> For example, one of the features enables collection of profiling data into a memory buffer that will grow indefinitely until you run out of memory.

### NixOS/Nix

We have a community-maintained flake which provides a devshell with required dependencies. Use `nix build` to build niri, and then run `./results/bin/niri`.

If you're not on NixOS, you may need [NixGL](https://github.com/nix-community/nixGL) to run the resulting binary:

```
nix run --impure github:guibou/nixGL -- ./results/bin/niri
```

### Packaging

The recommended way to package niri is so that it runs as a standalone desktop session.
To do that, put files into the correct directories according to this table.

| File | Destination |
| ---- | ----------- |
| `target/release/niri` | `/usr/bin/` |
| `resources/niri-session` | `/usr/bin/` |
| `resources/niri.desktop` | `/usr/share/wayland-sessions/` |
| `resources/niri-portals.conf` | `/usr/share/xdg-desktop-portal/` |
| `resources/niri.service` (systemd) | `/usr/lib/systemd/user/` |
| `resources/niri-shutdown.target` (systemd) | `/usr/lib/systemd/user/` |
| `resources/dinit/niri` (dinit) | `/usr/lib/dinit.d/user/` |
| `resources/dinit/niri-shutdown` (dinit) | `/usr/lib/dinit.d/user/` |

Doing this will make niri appear in GDM and other display managers.

### Manual Installation

If installing directly without a package, the recommended file destinations are slightly different.
In this case, put the files in the directories indicated in the table below.
These may vary depending on your distribution.

| File | Destination |
| ---- | ----------- |
| `target/release/niri` | `/usr/local/bin/` |
| `resources/niri-session` | `/usr/local/bin/` |
| `resources/niri.desktop`  | `/usr/local/share/wayland-sessions/` |
| `resources/niri-portals.conf` | `/usr/local/share/xdg-desktop-portal/` |
| `resources/niri.service` (systemd) | `/etc/systemd/user/` |
| `resources/niri-shutdown.target` (systemd) | `/etc/systemd/user/` |
| `resources/dinit/niri` (dinit) | `/etc/dinit.d/user/` |
| `resources/dinit/niri-shutdown` (dinit) | `/etc/dinit.d/user/` |

[Alacritty]: https://github.com/alacritty/alacritty
[fuzzel]: https://codeberg.org/dnkl/fuzzel
