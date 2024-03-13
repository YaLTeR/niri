<h1 align="center">niri</h1>
<p align="center">A scrollable-tiling Wayland compositor.</p>
<p align="center">
    <a href="https://matrix.to/#/#niri:matrix.org"><img alt="Matrix" src="https://img.shields.io/matrix/niri%3Amatrix.org?logo=matrix&label=matrix"></a>
    <a href="https://github.com/YaLTeR/niri/blob/main/LICENSE"><img alt="GitHub License" src="https://img.shields.io/github/license/YaLTeR/niri"></a>
    <a href="https://github.com/YaLTeR/niri/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/YaLTeR/niri?logo=github"></a>
</p>

![](https://github.com/YaLTeR/niri/assets/1794388/2b246c2c-7cf3-4a11-96eb-ad0c7f2f4ed6)

## About

Windows are arranged in columns on an infinite strip going to the right.
Opening a new window never causes existing windows to resize.

Every monitor has its own separate window strip.
Windows can never "overflow" onto an adjacent monitor.

Workspaces are dynamic and arranged vertically.
Every monitor has an independent set of workspaces, and there's always one empty workspace present all the way down.

The workspace arrangement is preserved across disconnecting and connecting monitors where it makes sense.
When a monitor disconnects, its workspaces will move to another monitor, but upon reconnection they will move back to the original monitor.

## Features

- Scrollable tiling
- Dynamic workspaces like in GNOME
- Built-in screenshot UI
- Monitor screencasting through xdg-desktop-portal-gnome
- Touchpad gestures
- Configurable layout: gaps, borders, struts, window sizes
- Live-reloading config

## Video Demo

https://github.com/YaLTeR/niri/assets/1794388/5d355694-7b06-4f00-8920-8dce54a8721c

## Status

A lot of the essential functionality is implemented, plus some goodies on top.
Feel free to give niri a try.
Have your [waybar]s and [fuzzel]s ready: niri is not a complete desktop environment.

Note that NVIDIA GPUs might have rendering issues.

## Inspiration

Niri is heavily inspired by [PaperWM] which implements scrollable tiling on top of GNOME Shell.

One of the reasons that prompted me to try writing my own compositor is being able to properly separate the monitors.
Being a GNOME Shell extension, PaperWM has to work against Shell's global window coordinate space to prevent windows from overflowing.

## Packages

There are several community-maintained distribution packages that you can use to install niri.
Here are some of them:

- Fedora COPR (I maintain this one myself): https://copr.fedorainfracloud.org/coprs/yalter/niri/
- AUR: [niri](https://aur.archlinux.org/packages/niri), [niri-bin](https://aur.archlinux.org/packages/niri-bin), [niri-git](https://aur.archlinux.org/packages/niri-git)
- NixOS Flake: https://github.com/sodiboo/niri-flake
- FreeBSD Ports: https://www.freshports.org/x11-wm/niri
- Gentoo GURU: https://gpo.zugaina.org/Overlays/guru/gui-wm/niri

## Building

First, install the dependencies for your distribution.

- Ubuntu 23.10:

    ```sh
    sudo apt-get install -y gcc clang libudev-dev libgbm-dev libxkbcommon-dev libegl1-mesa-dev libwayland-dev libinput-dev libdbus-1-dev libsystemd-dev libseat-dev libpipewire-0.3-dev libpango1.0-dev
    ```

- Fedora:

    ```sh
    sudo dnf install gcc libudev-devel libgbm-devel libxkbcommon-devel wayland-devel libinput-devel dbus-devel systemd-devel libseat-devel pipewire-devel pango-devel cairo-gobject-devel clang
    ```

Next, get latest stable Rust: https://rustup.rs/

Then, build niri with `cargo build --release`.

### NixOS/Nix

We have a community-maintained flake which provides a devshell with required dependencies. Use `nix build` to build niri, and then run `./results/bin/niri`.
    
If you're not on NixOS, you may need [NixGL](https://github.com/nix-community/nixGL) to run the resulting binary:

```
nix run --impure github:guibou/nixGL -- ./results/bin/niri
```

## Installation

The recommended way to install and run niri is as a standalone desktop session.
To do that, put files into the correct directories according to this table.

| File | Destination |
| ---- | ----------- |
| `target/release/niri` | `/usr/bin/` |
| `resources/niri-session` | `/usr/bin/` |
| `resources/niri.desktop` | `/usr/share/wayland-sessions/` |
| `resources/niri-portals.conf` | `/usr/share/xdg-desktop-portal/` |
| `resources/niri.service` | `/usr/lib/systemd/user/` |
| `resources/niri-shutdown.target` | `/usr/lib/systemd/user/` |

Doing this will make niri appear in GDM and, presumably, other display managers.

## Running

`cargo run --release`

Inside an existing desktop session, it will run in a window.
On a TTY, it will run natively.

To exit when running on a TTY, press <kbd>Super</kbd><kbd>Shift</kbd><kbd>E</kbd>.

### Session

If you followed the recommended installation steps above, niri should appear in your display manager.
Starting it from there will run niri as a desktop session.

The niri session will autostart apps through the systemd xdg-autostart target.
You can also autostart systemd services like [mako] by symlinking them into `$HOME/.config/systemd/user/niri.service.wants/`.
A step-by-step process for this is explained [on the wiki](https://github.com/YaLTeR/niri/wiki/Example-systemd-Setup).

Niri also works with some parts of xdg-desktop-portal-gnome.
In particular, it supports file choosers and monitor screencasting (e.g. to [OBS]).

[This wiki page](https://github.com/YaLTeR/niri/wiki/Important-Software) explains how to run important software required for normal desktop use, including portals.

### Xwayland

See [the wiki page](https://github.com/YaLTeR/niri/wiki/Xwayland) to learn how to use Xwayland with niri.

### IPC

You can communicate with the running niri instance over an IPC socket.
Check `niri msg --help` for available commands.

The `--json` flag prints the response in JSON, rather than formatted.
For example, `niri msg --json outputs`.

For programmatic access, check the [niri-ipc sub-crate](./niri-ipc/) which defines the types.
The communication over the IPC socket happens in JSON.

## Default Hotkeys

When running on a TTY, the Mod key is <kbd>Super</kbd>.
When running in a window, the Mod key is <kbd>Alt</kbd>.

The general system is: if a hotkey switches somewhere, then adding <kbd>Ctrl</kbd> will move the focused window or column there.

| Hotkey | Description |
| ------ | ----------- |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>/</kbd> | Show a list of important niri hotkeys |
| <kbd>Mod</kbd><kbd>T</kbd> | Spawn `alacritty` (terminal) |
| <kbd>Mod</kbd><kbd>D</kbd> | Spawn `fuzzel` (application launcher) |
| <kbd>Mod</kbd><kbd>Alt</kbd><kbd>L</kbd> | Spawn `swaylock` (screen locker) |
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
| <kbd>Mod</kbd><kbd>R</kbd> | Toggle between preset column widths |
| <kbd>Mod</kbd><kbd>F</kbd> | Maximize column |
| <kbd>Mod</kbd><kbd>C</kbd> | Center column within view |
| <kbd>Mod</kbd><kbd>-</kbd> | Decrease column width by 10% |
| <kbd>Mod</kbd><kbd>=</kbd> | Increase column width by 10% |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>-</kbd> | Decrease window height by 10% |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>=</kbd> | Increase window height by 10% |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>F</kbd> | Toggle full-screen on the focused window |
| <kbd>PrtSc</kbd> | Take an area screenshot. Select the area to screenshot with mouse, then press Space to save the screenshot, or Escape to cancel |
| <kbd>Alt</kbd><kbd>PrtSc</kbd> | Take a screenshot of the focused window to clipboard and to `~/Pictures/Screenshots/` |
| <kbd>Ctrl</kbd><kbd>PrtSc</kbd> | Take a screenshot of the focused monitor to clipboard and to `~/Pictures/Screenshots/` |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>E</kbd> | Exit niri |

## Configuration

Niri will load configuration from `$XDG_CONFIG_HOME/.config/niri/config.kdl` or `~/.config/niri/config.kdl`.
If this fails, it will load [the default configuration file](resources/default-config.kdl).
Please use the default configuration file as the starting point for your custom configuration.

Niri will live-reload most of the configuration settings, like key binds or gaps or output modes, as you change the config file.

## Contact

We have a Matrix chat, feel free to join and ask a question: https://matrix.to/#/#niri:matrix.org

[PaperWM]: https://github.com/paperwm/PaperWM
[mako]: https://github.com/emersion/mako
[OBS]: https://flathub.org/apps/com.obsproject.Studio
[waybar]: https://github.com/Alexays/Waybar
[fuzzel]: https://codeberg.org/dnkl/fuzzel

