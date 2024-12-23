### Overview

Key bindings are declared in the `binds {}` section of the config.

> [!NOTE]
> This is one of the few sections that *does not* get automatically filled with defaults if you omit it, so make sure to copy it from the default config.

Each bind is a hotkey followed by one action enclosed in curly brackets.
For example:

```kdl
binds {
    Mod+Left { focus-column-left; }
    Super+Alt+L { spawn "swaylock"; }
}
```

The hotkey consists of modifiers separated by `+` signs, followed by an XKB key name in the end.

Valid modifiers are:

- `Ctrl` or `Control`;
- `Shift`;
- `Alt`;
- `Super` or `Win`;
- `ISO_Level3_Shift` or `Mod5`—this is the AltGr key on certain layouts;
- `ISO_Level5_Shift`: can be used with an xkb lv5 option like `lv5:caps_switch`;
- `Mod`.

`Mod` is a special modifier that is equal to `Super` when running niri on a TTY, and to `Alt` when running niri as a nested winit window.
This way, you can test niri in a window without causing too many conflicts with the host compositor's key bindings.
For this reason, most of the default keys use the `Mod` modifier.

> [!TIP]
> To find an XKB name for a particular key, you may use a program like [`wev`](https://git.sr.ht/~sircmpwn/wev).
>
> Open it from a terminal and press the key that you want to detect.
> In the terminal, you will see output like this:
>
> ```
> [14:     wl_keyboard] key: serial: 757775; time: 44940343; key: 113; state: 1 (pressed)
>                       sym: Left         (65361), utf8: ''
> [14:     wl_keyboard] key: serial: 757776; time: 44940432; key: 113; state: 0 (released)
>                       sym: Left         (65361), utf8: ''
> [14:     wl_keyboard] key: serial: 757777; time: 44940753; key: 114; state: 1 (pressed)
>                       sym: Right        (65363), utf8: ''
> [14:     wl_keyboard] key: serial: 757778; time: 44940846; key: 114; state: 0 (released)
>                       sym: Right        (65363), utf8: ''
> ```
>
> Here, look at `sym: Left` and `sym: Right`: these are the key names.
> I was pressing the left and the right arrow in this example.

<sup>Since: 0.1.8</sup> Binds will repeat by default (i.e. holding down a bind will make it trigger repeatedly).
You can disable that for specific binds with `repeat=false`:

```kdl
binds {
    Mod+T repeat=false { spawn "alacritty"; }
}
```

Binds can also have a cooldown, which will rate-limit the bind and prevent it from repeatedly triggering too quickly.

```kdl
binds {
    Mod+T cooldown-ms=500 { spawn "alacritty"; }
}
```

This is mostly useful for the scroll bindings.

### Scroll Bindings

You can bind mouse wheel scroll ticks using the following syntax.
These binds will change direction based on the `natural-scroll` setting.

```kdl
binds {
    Mod+WheelScrollDown cooldown-ms=150 { focus-workspace-down; }
    Mod+WheelScrollUp   cooldown-ms=150 { focus-workspace-up; }
    Mod+WheelScrollRight                { focus-column-right; }
    Mod+WheelScrollLeft                 { focus-column-left; }
}
```

Similarly, you can bind touchpad scroll "ticks".
Touchpad scrolling is continuous, so for these binds it is split into discrete intervals based on distance travelled.

These binds are also affected by touchpad's `natural-scroll`, so these example binds are "inverted", since niri has `natural-scroll` enabled for touchpads by default.

```kdl
binds {
    Mod+TouchpadScrollDown { spawn "wpctl" "set-volume" "@DEFAULT_AUDIO_SINK@" "0.02+"; }
    Mod+TouchpadScrollUp   { spawn "wpctl" "set-volume" "@DEFAULT_AUDIO_SINK@" "0.02-"; }
}
```

Both mouse wheel and touchpad scroll binds will prevent applications from receiving any scroll events when their modifiers are held down.
For example, if you have a `Mod+WheelScrollDown` bind, then while holding `Mod`, all mouse wheel scrolling will be consumed by niri.

### Actions

Every action that you can bind is also available for programmatic invocation via `niri msg action`.
Run `niri msg action` to get a full list of actions along with their short descriptions.

Here are a few actions that benefit from more explanation.

#### `spawn`

Run a program.

`spawn` accepts a path to the program binary as the first argument, followed by arguments to the program.
For example:

```kdl
binds {
    // Run alacritty.
    Mod+T { spawn "alacritty"; }

    // Run `wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.1+`.
    XF86AudioRaiseVolume { spawn "wpctl" "set-volume" "@DEFAULT_AUDIO_SINK@" "0.1+"; }
}
```

> [!TIP]
>
> <sup>Since: 0.1.5</sup>
>
> Spawn bindings have a special `allow-when-locked=true` property that makes them work even while the session is locked:
>
> ```kdl
> binds {
>     // This mute bind will work even when the session is locked.
>     XF86AudioMute allow-when-locked=true { spawn "wpctl" "set-mute" "@DEFAULT_AUDIO_SINK@" "toggle"; }
> }
> ```

Currently, niri *does not* use a shell to run commands, which means that you need to manually separate arguments.

```kdl
binds {
    // Correct: every argument is in its own quotes.
    Mod+T { spawn "alacritty" "-e" "/usr/bin/fish"; }

    // Wrong: will interpret the whole `alacritty -e /usr/bin/fish` string as the binary path.
    Mod+D { spawn "alacritty -e /usr/bin/fish"; }

    // Wrong: will pass `-e /usr/bin/fish` as one argument, which alacritty won't understand.
    Mod+Q { spawn "alacritty" "-e /usr/bin/fish"; }
}
```

This also means that you cannot expand environment variables or `~`.
If you need this, you can run the command through a shell manually.

```kdl
binds {
    // Wrong: no shell expansion here. These strings will be passed literally to the program.
    Mod+T { spawn "grim" "-o" "$MAIN_OUTPUT" "~/screenshot.png"; }

    // Correct: run this through a shell manually so that it can expand the arguments.
    // Note that the entire command is passed as a SINGLE argument,
    // because shell will do its own argument splitting by whitespace.
    Mod+D { spawn "sh" "-c" "grim -o $MAIN_OUTPUT ~/screenshot.png"; }

    // You can also use a shell to run multiple commands,
    // use pipes, process substitution, and so on.
    Mod+Q { spawn "sh" "-c" "notify-send clipboard \"$(wl-paste)\""; }
}
```

As a special case, niri will expand `~` to the home directory *only* at the beginning of the program name.

```kdl
binds {
    // This will work: one ~ at the very beginning.
    Mod+T { spawn "~/scripts/do-something.sh"; }
}
```

#### `quit`

Exit niri after showing a confirmation dialog to avoid accidentally triggering it.

```kdl
binds {
    Mod+Shift+E { quit; }
}
```

If you want to skip the confirmation dialog, set the flag like so:

```kdl
binds {
    Mod+Shift+E { quit skip-confirmation=true; }
}
```

#### `do-screen-transition`

<sup>Since: 0.1.6</sup>

Freeze the screen for a brief moment then crossfade to the new contents.

```kdl
binds {
    Mod+Return { do-screen-transition; }
}
```

This action is mainly useful to trigger from scripts changing the system theme or style (between light and dark for example).
It makes transitions like this, where windows change their style one by one, look smooth and synchronized.

For example, using the GNOME color scheme setting:

```shell
niri msg action do-screen-transition
dconf write /org/gnome/desktop/interface/color-scheme "\"prefer-dark\""
```

By default, the screen is frozen for 250 ms to give windows time to redraw, before the crossfade.
You can set this delay like this:

```kdl
binds {
    Mod+Return { do-screen-transition delay-ms=100; }
}
```

Or, in scripts:

```shell
niri msg action do-screen-transition --delay-ms 100
```
