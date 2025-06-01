When starting niri from a display manager like GDM, or otherwise through the `niri-session` binary, it runs as a systemd service.
This provides the necessary systemd integration to run programs like `mako` and services like `xdg-desktop-portal` bound to the graphical session.

Here's an example on how you might set up [`mako`](https://github.com/emersion/mako), [`waybar`](https://github.com/Alexays/Waybar), [`swaybg`](https://github.com/swaywm/swaybg) and [`swayidle`](https://github.com/swaywm/swayidle) to run as systemd services with niri.
Unlike [`spawn-at-startup`](./Configuration:-Miscellaneous.md#spawn-at-startup), this lets you easily monitor their status, output, and restart or reload them.

1. Install them, i.e. `sudo dnf install mako waybar swaybg swayidle`
2. `mako` and `waybar` provide systemd units. To add them to your niri session run: 
    ```
    systemctl --user add-wants niri.service mako.service
    systemctl --user add-wants niri.service waybar.service
    ```
    This wil provide links in `$HOME/.config/systemd/user/niri.service.wants` or for XDG configurations `$XDG_CONFIG_HOME/systemd/user/niri.service.wants`.

    **Next** we willl create two example units for `swaybg` and `swayidle`. These will be saved in `$HOME/.config/system/user` or for XDG configurations, `$XDG_CONFIG_HOME/systemd/user`;
4. `swaybg` does not provide a systemd unit. Create `swaybg.service` from one
    of the above directories.

    ```
    [Unit]
    PartOf=graphical-session.target
    After=graphical-session.target
    Requisite=graphical-session.target

    [Service]
    ExecStart=/usr/bin/swaybg -m fill -i "%h/Pictures/LakeSide.png"
    Restart=on-failure
    ```

    Replace the image path with the one you want. `%h` is expanded to your home directory.

    Save your changes, then run
    ```systemctl --user daemon-reload```
    so systemd picks up the file changes. Now,

    ```
    systemctl --user add-wants niri.service swaybg.service
    ```
    This will add a dependancy `swaybg.service` to the niri session.
5. Similarly, for `swayidle` we will also make our own. Create a `swayidle.service` _in one of the directories from **step 2***_.

    ```
    [Unit]
    PartOf=graphical-session.target
    After=graphical-session.target
    Requisite=graphical-session.target

    [Service]
    ExecStart=/usr/bin/swayidle -w timeout 601 'niri msg action power-off-monitors' timeout 600 'swaylock -f' before-sleep 'swaylock -f'
    Restart=on-failure
    ```

    Save the file and run `systemctl --user daemon-reload`. Now,

    ```
    systemctl --user add-wants niri.service swayidle.service
    ```

To stop using a service on a niri session startup remove their link from `$HOME/.config/systemd/user/niri.service.wants` or `$XDG_CONFIG_HOME/systemd/user/niri.service.wants` for XDG configuration. Then, do a `systemctl --user daemon-reload`.

**That's it!** Now these utilities will be started together with the niri session and stopped when it exits.

You can also restart them with a command like `systemctl --user restart waybar.service`, for example after editing their config files.

### Running Programs Across Logout

When running niri as a session, exiting it (logging out) will kill all programs that you've started within. However, sometimes you want a program, like `tmux`, `dtach` or similar, to persist in this case. To do this, run it in a transient systemd scope:

```
systemd-run --user --scope tmux new-session
```
