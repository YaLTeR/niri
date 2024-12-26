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

![](https://github.com/YaLTeR/niri/assets/1794388/52c799a1-77ec-455f-b4aa-f3236a144964)

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

A lot of the essential functionality is implemented, plus some goodies on top.
Feel free to give niri a try: follow the instructions on the [Getting Started](https://github.com/YaLTeR/niri/wiki/Getting-Started) wiki page.
Have your [waybar]s and [fuzzel]s ready: niri is not a complete desktop environment.

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
