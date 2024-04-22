<h1 align="center">niri</h1>
<p align="center">A scrollable-tiling Wayland compositor.</p>
<p align="center">
    <a href="https://matrix.to/#/#niri:matrix.org"><img alt="Matrix" src="https://img.shields.io/matrix/niri%3Amatrix.org?logo=matrix&label=matrix"></a>
    <a href="https://github.com/YaLTeR/niri/blob/main/LICENSE"><img alt="GitHub License" src="https://img.shields.io/github/license/YaLTeR/niri"></a>
    <a href="https://github.com/YaLTeR/niri/releases"><img alt="GitHub Release" src="https://img.shields.io/github/v/release/YaLTeR/niri?logo=github"></a>
</p>

<p align="center">
    <a href="https://github.com/YaLTeR/niri/wiki/Getting-Started">Getting Started</a> | <a href="https://github.com/YaLTeR/niri/wiki/Configuration:-Overview">Configuration</a>
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
    - You can [block out](https://github.com/YaLTeR/niri/wiki/Configuration:-Window-Rules#block-out-from) sensitive windows from screencasts
- [Touchpad gestures](https://github.com/YaLTeR/niri/assets/1794388/946a910e-9bec-4cd1-a923-4a9421707515)
- Configurable layout: gaps, borders, struts, window sizes
- [Animations](https://github.com/YaLTeR/niri/assets/1794388/ce178da2-af9e-4c51-876f-8709c241d95e)
- Live-reloading config

## Video Demo

https://github.com/YaLTeR/niri/assets/1794388/bce834b0-f205-434e-a027-b373495f9729

## Status

A lot of the essential functionality is implemented, plus some goodies on top.
Feel free to give niri a try: follow the instructions on the [Getting Started](https://github.com/YaLTeR/niri/wiki/Getting-Started) wiki page.
Have your [waybar]s and [fuzzel]s ready: niri is not a complete desktop environment.

Note that NVIDIA GPUs may have issues.

## Inspiration

Niri is heavily inspired by [PaperWM] which implements scrollable tiling on top of GNOME Shell.

One of the reasons that prompted me to try writing my own compositor is being able to properly separate the monitors.
Being a GNOME Shell extension, PaperWM has to work against Shell's global window coordinate space to prevent windows from overflowing.

## Contact

We have a Matrix chat, feel free to join and ask a question: https://matrix.to/#/#niri:matrix.org

[PaperWM]: https://github.com/paperwm/PaperWM
[waybar]: https://github.com/Alexays/Waybar
[fuzzel]: https://codeberg.org/dnkl/fuzzel

