### Overview

Switch event bindings are declared in the `switch-events {}` section of the config.

```kdl
switch-events {
    lid-close { spawn "bash" "-c" "niri msg output \"eDP-1\" off"; }
    lid-open { spawn "bash" "-c" "niri msg output \"eDP-1\" on"; }
    tablet-mode-on { spawn "bash" "-c" "gsettings set org.gnome.desktop.a11y.applications screen-keyboard-enabled true"; }
    tablet-mode-off { spawn "bash" "-c" "gsettings set org.gnome.desktop.a11y.applications screen-keyboard-enabled false"; }
}
```

Currently only `spawn` actions are supported.

> [!NOTE]
> In contrast to key bindings, switch event bindings are *always* executed, independent of the current lock state.