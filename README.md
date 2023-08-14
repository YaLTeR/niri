# niri

The beginnings of a scrollable-tiling Wayland compositor.

## Status

Heavily work in progress.
The occasional thing works, but likely is in a half-broken state.

## Idea

This section describes the goals I'm working towards.
Many things don't work as written yet.

Niri implements scrollable tiling, heavily inspired by [PaperWM].
Windows are arranged in columns on an infinite strip going to the right.
Every column takes up as much height as possible, spread between its windows.

![](https://github.com/YaLTeR/niri/assets/1794388/b734da07-301a-452b-b201-d4789a3eca60)

With multiple monitors, every monitor has its own separate window strip.
Windows can never "overflow" to an adjacent monitor.

This is one of the reasons that prompted me to try writing my own compositor.
PaperWM is a solid implementation that I use every day, but, being a GNOME Shell extension, it has to work around Shell's global window coordinate space to prevent windows from overflowing.

Niri also has dynamic workspaces which work similar to GNOME Shell.
Since windows go left-to-right horizontally, workspaces are arranged vertically.
Every monitor has an independent set of workspaces, and there's always one empty workspace present all the way down.

Niri tries to preserve the workspace arrangement as much as possible upon disconnecting and connecting monitors.
When a monitor disconnects, its workspaces will move to another monitor, but upon reconnection they will move back to the original monitor where it makes sense.

## Running

`cargo run -- -- alacritty`

Inside a desktop session, it will run in a window.
On a TTY, it will run natively.

To exit when running on a TTY, press <kbd>Super</kbd><kbd>Shift</kbd><kbd>E</kbd>.

## Hotkeys

When running on a TTY, the Mod key is <kbd>Super</kbd>.
When running in a window, the Mod key is <kbd>Alt</kbd>.

The general system is: if a hotkey switches somewhere, then adding <kbd>Ctrl</kbd> will move the focused window or column there.

| Hotkey | Description |
| ------ | ----------- |
| <kbd>Mod</kbd><kbd>T</kbd> | Spawn `alacritty` |
| <kbd>Mod</kbd><kbd>Q</kbd> | Close the focused window |
| <kbd>Mod</kbd><kbd>H</kbd> or <kbd>Mod</kbd><kbd>←</kbd> | Focus the window to the left |
| <kbd>Mod</kbd><kbd>L</kbd> or <kbd>Mod</kbd><kbd>→</kbd> | Focus the window to the right |
| <kbd>Mod</kbd><kbd>J</kbd> or <kbd>Mod</kbd><kbd>↓</kbd> | Focus the window below in a column |
| <kbd>Mod</kbd><kbd>K</kbd> or <kbd>Mod</kbd><kbd>↑</kbd> | Focus the window above in a column |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>H</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>←</kbd> | Move the focused column to the left |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>L</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>→</kbd> | Move the focused column to the right |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>J</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>↓</kbd> | Move the focused window below in a column |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>K</kbd> or <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>↑</kbd> | Move the focused window above in a column |
| <kbd>Mod</kbd><kbd>U</kbd> | Switch to the workspace below |
| <kbd>Mod</kbd><kbd>I</kbd> | Switch to the workspace above |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>U</kbd> | Move the focused window to the workspace below |
| <kbd>Mod</kbd><kbd>Ctrl</kbd><kbd>I</kbd> | Move the focused window to the workspace above |
| <kbd>Mod</kbd><kbd>,</kbd> | Consume the window to the right into the focused column |
| <kbd>Mod</kbd><kbd>.</kbd> | Expel the focused window into its own column |
| <kbd>Mod</kbd><kbd>R</kbd> | Toggle between preset window widths |
| <kbd>Mod</kbd><kbd>Shift</kbd><kbd>E</kbd> | Exit niri |

[PaperWM]: https://github.com/paperwm/PaperWM

