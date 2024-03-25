Since niri is not a complete desktop environment, you will very likely want to run the following software to make sure that other apps work fine.

### Notification Daemon

Many apps need one. For example, [mako](https://github.com/emersion/mako) works well. Use [a systemd setup](https://github.com/YaLTeR/niri/wiki/Example-systemd-Setup) or `spawn-at-startup`.

### Portals

These provide a cross-desktop API for apps to use for various things like file pickers or UI settings. Flatpak apps in particular require working portals.

Portals **require** [running niri as a session](https://github.com/YaLTeR/niri#session), which means through the `niri-session` script or from a display manager. You will want the following portals installed:

* `xdg-desktop-portal-gtk`: implements most of the basic functionality, this is the "default fallback portal".
* `xdg-desktop-portal-gnome`: required for screencasting support.
* `gnome-keyring`: implements the Secret portal, required for certain apps to work.

Then systemd should start them on-demand automatically. These particular portals are configured in `niri-portals.conf` which [must be installed](https://github.com/YaLTeR/niri#installation) in the correct location.

Since we're using `xdg-desktop-portal-gnome`, Flatpak apps will read the GNOME UI settings. For example, to enable the dark style, run:

```
dconf write /org/gnome/desktop/interface/color-scheme '"prefer-dark"'
```

### Authentication Agent

Required when apps need to ask for root permissions. Something like `plasma-polkit-agent` works fine. Start it [with systemd](https://github.com/YaLTeR/niri/wiki/Example-systemd-Setup) or with `spawn-at-startup`.

Note that to start `plasma-polkit-agent` with systemd on Fedora, you'll need to override its systemd service to add the correct dependency. Run:

```
systemctl --user edit --full plasma-polkit-agent.service
```

Then add `After=graphical-session.target`.