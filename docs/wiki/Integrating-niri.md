This page contains various bits of information helpful for integrating niri in a distribution.
First, for creating a niri package, see the [Packaging](./Packaging-niri.md) page.

### Configuration

Niri will load configuration from `$XDG_CONFIG_HOME/niri/config.kdl` or `~/.config/niri/config.kdl`, falling back to `/etc/niri/config.kdl`.
If both of these files are missing, niri will create `$XDG_CONFIG_HOME/niri/config.kdl` with the contents of [the default configuration file](https://github.com/YaLTeR/niri/blob/main/resources/default-config.kdl), which are embedded into the niri binary at build time.

This means that you can customize your distribution defaults by creating `/etc/niri/config.kdl`.
When this file is present, niri *will not* automatically create a config at `~/.config/niri/`, so you'll need to direct your users how to do it themselves.

Keep in mind that we update the default config in new releases, so if you have a custom `/etc/niri/config.kdl`, you likely want to inspect and apply the relevant changes too.

Splitting the niri config file into multiple files, or includes, are not supported yet.

### Xwayland

Xwayland is required for running X11 apps and games, and also the Orca screen reader.

<sup>Since: next release</sup> Niri integrates with [xwayland-satellite](https://github.com/Supreeeme/xwayland-satellite) out of the box.
The integration requires xwayland-satellite >= 0.7 available in `$PATH`.
Please consider making niri depend on (or at least recommend) the xwayland-satellite package.

You can change the path where niri looks for xwayland-satellite using the [`xwayland-satellite` top-level option](./Configuration:-Miscellaneous.md#xwayland-satellite).

### Keyboard layout

<sup>Since: next release</sup> By default (unless [manually configured](./Configuration:-Input.md#layout) otherwise), niri reads keyboard layout settings from systemd-localed at `org.freedesktop.locale1` over D-Bus.
Make sure your system installer sets the keyboard layout via systemd-localed, and niri should pick it up.

### Autostart

Niri works with the normal systemd autostart.
The default [niri.service](https://github.com/YaLTeR/niri/blob/main/resources/niri.service) brings up `graphical-session.target` as well as `xdg-desktop-autostart.target`.

To make a program run at niri startup without editing the niri config, you can either link its .desktop to `~/.config/autostart/`, or use a .service file with `WantedBy=graphical-session.target`.
See the [example systemd setup](./Example-systemd-Setup.md) page for some examples.

If this is inconvenient, you can also add [`spawn-at-startup`](./Configuration:-Miscellaneous.md#spawn-at-startup) lines in the niri config.

### Screen readers

<sup>Since: next release</sup> Niri works with the [Orca](https://orca.gnome.org) screen reader.
Please see the [Accessibility](./Accessibility.md) page for details and advice for accessibility-focused distributions.
