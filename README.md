<h1 align="center">niri</h1>
<p align="center">A scrollable-tiling Wayland compositor.</p>
<p align="center">
    <a href="https://matrix.to/#/#niri:matrix.org"><img alt="Matrix" src="https://img.shields.io/badge/matrix-%23niri-blue?logo=matrix"></a>
    <a href="https://github.com/YaLTeR/niri/blob/main/LICENSE"><img alt="GitHub License" src="https://img.shields.io/github/license/YaLTeR/niri"></a>
    <a href="https://github.com/YaLTeR/niri/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/YaLTeR/niri?logo=github"></a>
</p>

<p align="center">
    <a href="https://github.com/YaLTeR/niri/wiki/Getting-Started">Getting Started</a> | <a href="https://github.com/YaLTeR/niri/wiki/Configuration:-Overview">Configuration</a> | <a href="https://github.com/YaLTeR/niri/discussions/325">Setup&nbsp;Showcase</a>
</p>

![niri with a few windows open](https://github.com/user-attachments/assets/d142e57d-a25d-4ddb-ab46-311417458211)

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

- Built from the ground up for scrollable tiling
- Dynamic workspaces like in GNOME
- Built-in screenshot UI
- Monitor and window screencasting through xdg-desktop-portal-gnome
    - You can [block out](https://github.com/YaLTeR/niri/wiki/Configuration:-Window-Rules#block-out-from) sensitive windows from screencasts
- [Touchpad](https://github.com/YaLTeR/niri/assets/1794388/946a910e-9bec-4cd1-a923-4a9421707515) and [mouse](https://github.com/YaLTeR/niri/assets/1794388/8464e65d-4bf2-44fa-8c8e-5883355bd000) gestures
- Configurable layout: gaps, borders, struts, window sizes
- [Gradient borders](https://github.com/YaLTeR/niri/wiki/Configuration:-Layout#gradients) with Oklab and Oklch support
- [Animations](https://github.com/YaLTeR/niri/assets/1794388/ce178da2-af9e-4c51-876f-8709c241d95e) with support for [custom shaders](https://github.com/YaLTeR/niri/assets/1794388/27a238d6-0a22-4692-b794-30dc7a626fad)
- Live-reloading config

## Video Demo

https://github.com/YaLTeR/niri/assets/1794388/bce834b0-f205-434e-a027-b373495f9729

## Status

Niri is stable for day-to-day use and does most things expected of a Wayland compositor.
Many people are daily-driving niri, and are happy to help in our [Matrix channel].

Give it a try!
Follow the instructions on the [Getting Started](https://github.com/YaLTeR/niri/wiki/Getting-Started) wiki page.
Have your [waybar]s and [fuzzel]s ready: niri is not a complete desktop environment.

Here are some points you may have questions about:

- **Multi-monitor**: yes, a core part of the design from the very start. Mixed DPI works.
- **Fractional scaling**: yes, plus all niri UI stays pixel-perfect.
- **NVIDIA**: seems to work fine.
- **Floating windows**: yes, starting from niri 25.01.
- **Input devices**: niri supports tablets, touchpads, and touchscreens.
You can map the tablet to a specific monitor, or use [OpenTabletDriver].
We have touchpad gestures, but no touchscreen gestures yet.
- **Wlr protocols**: yes, we have most of the important ones like layer-shell, gamma-control, screencopy.
You can check on [wayland.app](https://wayland.app) at the bottom of each protocol's page.
- **Performance**: while I run niri on beefy machines, I try to stay conscious of performance.
I've seen someone use it fine on an Eee PC 900 from 2008, of all things.
- **Xwayland**: no built-in support, but xwayland-satellite is [easy to set up](https://github.com/YaLTeR/niri/wiki/Xwayland#using-xwayland-satellite) and works very well.
    - Steam and games, including Proton: work perfectly through xwayland-satellite.
    - JetBrains IDEs, Ghidra: work well through xwayland-satellite.
    - Discord and other Electron apps: work well through xwayland-satellite.
    - Chromium and VSCode: work perfectly natively on Wayland with the right flags.
    - X11 apps that want to position windows or bars at specific screen coordinates: won't work well; you can run them in a nested compositor like [labwc](https://github.com/YaLTeR/niri/wiki/Xwayland#using-the-labwc-wayland-compositor) or [rootful Xwayland](https://github.com/YaLTeR/niri/wiki/Xwayland#directly-running-xwayland-in-rootful-mode).
    - Display scaling (integer or fractional) will make X11 apps look blurry; this needs to be supported in xwayland-satellite.
    For games, you can run them in [gamescope] at native resolution, even with display scaling.

## Inspiration

Niri is heavily inspired by [PaperWM] which implements scrollable tiling on top of GNOME Shell.

One of the reasons that prompted me to try writing my own compositor is being able to properly separate the monitors.
Being a GNOME Shell extension, PaperWM has to work against Shell's global window coordinate space to prevent windows from overflowing.

## Tile Scrollably Elsewhere

Here are some other projects which implement a similar workflow:

- [PaperWM]: scrollable tiling on top of GNOME Shell.
- [karousel]: scrollable tiling on top of KDE.
- [papersway]: scrollable tiling on top of sway/i3.
- [hyprscroller] and [hyprslidr]: scrollable tiling on top of Hyprland.
- [PaperWM.spoon]: scrollable tiling on top of macOS.

## Contact

We have a Matrix chat, feel free to join and ask a question: https://matrix.to/#/#niri:matrix.org

[PaperWM]: https://github.com/paperwm/PaperWM
[waybar]: https://github.com/Alexays/Waybar
[fuzzel]: https://codeberg.org/dnkl/fuzzel
[karousel]: https://github.com/peterfajdiga/karousel
[papersway]: https://spwhitton.name/tech/code/papersway/
[hyprscroller]: https://github.com/dawsers/hyprscroller
[hyprslidr]: https://gitlab.com/magus/hyprslidr
[PaperWM.spoon]: https://github.com/mogenson/PaperWM.spoon
[Matrix channel]: https://matrix.to/#/#niri:matrix.org
[OpenTabletDriver]: https://opentabletdriver.net/
[gamescope]: https://github.com/ValveSoftware/gamescope
