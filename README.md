# niri

A scrollable-tiling Wayland compositor.

![](https://github.com/YaLTeR/niri/assets/1794388/e35fd9e1-105b-4bd5-94c9-207fd6fb3c18)

## Status

A lot of the essential functionality is implemented, plus some goodies on top.
Feel free to give niri a try.
Have your waybars and fuzzels ready: niri is not a complete desktop environment.

https://github.com/YaLTeR/niri/assets/1794388/5d355694-7b06-4f00-8920-8dce54a8721c

## Idea

Niri implements scrollable tiling, heavily inspired by [PaperWM].
Windows are arranged in columns on an infinite strip going to the right.
Every column takes up a full monitor worth of height, divided among its windows.

With multiple monitors, every monitor has its own separate window strip.
Windows can never "overflow" onto an adjacent monitor.

This is one of the reasons that prompted me to try writing my own compositor.
PaperWM is a solid implementation, but, being a GNOME Shell extension, it has to work around Shell's global window coordinate space to prevent windows from overflowing.

Niri also has dynamic workspaces which work similar to GNOME Shell.
Since windows go left-to-right horizontally, workspaces are arranged vertically.
Every monitor has an independent set of workspaces, and there's always one empty workspace present all the way down.

Niri tries to preserve the workspace arrangement as much as possible upon disconnecting and connecting monitors.
When a monitor disconnects, its workspaces will move to another monitor, but upon reconnection they will move back to the original monitor.

## Building

First, install the dependencies for your distribution.

- Ubuntu:

    ```sh
    sudo apt-get install -y software-properties-common
    sudo add-apt-repository -y ppa:pipewire-debian/pipewire-upstream
    sudo apt-get update -y
    sudo apt-get install -y libudev-dev libgbm-dev libxkbcommon-dev libegl1-mesa-dev libwayland-dev libinput-dev libdbus-1-dev libsystemd-dev libseat-dev libpipewire-0.3-dev
    ```

- Fedora:

    ```sh
    sudo dnf install gcc libudev-devel libgbm-devel libxkbcommon-devel wayland-devel libinput-devel dbus-devel systemd-devel libseat-devel pipewire-devel clang
    ```

Next, build niri with `cargo build --release`.

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

Niri also works with some parts of xdg-desktop-portal-gnome.
In particular, it supports file choosers and monitor screencasting (e.g. to [OBS]).

## Default Hotkeys

When running on a TTY, the Mod key is <kbd>Super</kbd>.
When running in a window, the Mod key is <kbd>Alt</kbd>.

The general system is: if a hotkey switches somewhere, then adding <kbd>Ctrl</kbd> will move the focused window or column there.

| Hotkey | Description |
| ------ | ----------- |
| <kbd>Mod</kbd><kbd>T</kbd> | Spawn `alacritty` |
| <kbd>Mod</kbd><kbd>Q</kbd> | Close the focused window |
| <kbd>Mod</kbd><kbd>H</kbd> or <kbd>Mod</kbd><kbd>←</kbd> | Focus the column to the left |
| <kbd>Mod</kbd><kbd>L</kbd> or <kbd>Mod</kbd><kbd>→</kbd> | Focus the column to the right |
| <kbd>Mod</kbd><kbd>J</kbd> or <kbd>Mod</kbd><kbd>↓</kbd> | Focus the window below in a column |
| <kbd>Mod</kbd><kbd>K</kbd> or <kbd>Mod</kbd><kbd>↑</kbd> | Focus the window above in a column |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>H</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>←</kbd> | Move the focused column to the left |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>L</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>→</kbd> | Move the focused column to the right |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>J</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>↓</kbd> | Move the focused window below in a column |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>K</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>↑</kbd> | Move the focused window above in a column |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>H</kbd><kbd>J</kbd><kbd>K</kbd><kbd>L</kbd> or <kbd>Mod</kbd><kbd>Shift</kbd><kbd>←</kbd><kbd>↓</kbd><kbd>↑</kbd><kbd>→</kbd> | Focus the monitor to the side |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>H</kbd><kbd>J</kbd><kbd>K</kbd><kbd>L</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>←</kbd><kbd>↓</kbd><kbd>↑</kbd><kbd>→</kbd> | Move the focused window to the monitor to the side |
| <kbd>Mod</kbd><kbd>U</kbd> or <kbd>Mod</kbd><kbd>PageDown</kbd> | Switch to the workspace below |
| <kbd>Mod</kbd><kbd>I</kbd> or <kbd>Mod</kbd><kbd>PageUp</kbd> | Switch to the workspace above |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>U</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>PageDown</kbd> | Move the focused window to the workspace below |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>I</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>PageUp</kbd> | Move the focused window to the workspace above |
| <kbd>Mod</kbd><kbd>1</kbd>–<kbd>9</kbd> | Switch to a workspace by index |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>1</kbd>–<kbd>9</kbd> | Move the focused window to a workspace by index |
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
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>T</kbd> | Toggle debug tinting of rendered elements |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>E</kbd> | Exit niri |

## Configuration

Niri will load configuration from `$XDG_CONFIG_HOME/.config/niri/config.kdl` or `~/.config/niri/config.kdl`.
If this fails, it will load [the default configuration file](resources/default-config.kdl).
Please use the default configuration file as the starting point for your custom configuration.

[PaperWM]: https://github.com/paperwm/PaperWM
[mako]: https://github.com/emersion/mako
[OBS]: https://flathub.org/apps/com.obsproject.Studio

