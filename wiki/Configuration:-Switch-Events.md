### Overview

Switch event bindings are declared in the `switch-events {}` section of the config.

Here are all the events that you can bind at a glance:

```kdl
switch-events {
    lid-close { spawn "bash" "-c" "niri msg output \"eDP-1\" off"; }
    lid-open { spawn "bash" "-c" "niri msg output \"eDP-1\" on"; }
    tablet-mode-on { spawn "bash" "-c" "gsettings set org.gnome.desktop.a11y.applications screen-keyboard-enabled true"; }
    tablet-mode-off { spawn "bash" "-c" "gsettings set org.gnome.desktop.a11y.applications screen-keyboard-enabled false"; }
}
```

The syntax is similar to key bindings.
Currently only the `spawn` action are supported.

> [!NOTE]
> In contrast to key bindings, switch event bindings are *always* executed, even when the session is locked.

### `lid-close`, `lid-open`

These events correspond to closing and opening of the laptop lid.

You could use them to turn the laptop internal monitor off and on (until niri gets this functionality built-in).

```kdl
switch-events {
    lid-close { spawn "bash" "-c" "niri msg output \"eDP-1\" off"; }
    lid-open { spawn "bash" "-c" "niri msg output \"eDP-1\" on"; }
}
```

### `tablet-mode-on`, `tablet-mode-off`

These events trigger when a convertible laptop goes into or out of tablet mode.
In tablet mode, the keyboard and mouse are usually inaccessible, so you can use these events to activate the on-screen keyboard.

```kdl
switch-events {
    tablet-mode-on { spawn "bash" "-c" "gsettings set org.gnome.desktop.a11y.applications screen-keyboard-enabled true"; }
    tablet-mode-off { spawn "bash" "-c" "gsettings set org.gnome.desktop.a11y.applications screen-keyboard-enabled false"; }
}
```
