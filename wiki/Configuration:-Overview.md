### Loading

Niri will load configuration from `$XDG_CONFIG_HOME/.config/niri/config.kdl` or `~/.config/niri/config.kdl`.
If that file is missing, niri will create it with the contents of [the default configuration file](../resources/default-config.kdl).
Please use the default configuration file as the starting point for your custom configuration.

The configuration is live-reloaded.
Simply edit and save the config file, and your changes will be applied.
This includes key bindings, output settings like mode, window rules, and everything else.

You can run `niri validate` to parse the config and see any errors.

To use a different config file path, pass it in the `--config` or `-c` argument to `niri`.

### Syntax

The config is written in [KDL].

#### Comments

Lines starting with `//` are comments; they are ignored.

Also, you can put `/-` in front of a section to comment out the entire section:

```
/-output "eDP-1" {
    everything inside here
    is ignored
}
```

#### Flags

Toggle options in niri are commonly represented as flags.
Writing out the flag enables it, and omitting it or commenting it out disables it.
For example:

```
// "Focus follows mouse" is enabled.
input {
    focus-follows-mouse

    // Other settings...
}
```

```
// "Focus follows mouse" is disabled.
input {
    // focus-follows-mouse

    // Other settings...
}
```

#### Sections

Most sections cannot be repeated. For example:

```
// This is valid: every section appears once.
input {
    keyboard {
        // ...
    }

    touchpad {
        // ...
    }
}
```

```
// This is NOT valid: input section appears twice.
input {
    keyboard {
        // ...
    }
}

input {
    touchpad {
        // ...
    }
}
```

Exceptions are, for example, sections that configure different devices by name:

```
output "eDP-1" {
    // ...
}

// This is valid: this section configures a different output.
output "HDMI-A-1" {
    // ...
}

// This is NOT valid: "eDP-1" already appeared above.
// It will either throw a config parsing error, or otherwise not work.
output "eDP-1" {
    // ...
}
```

### Defaults

Omitting most of the sections of the config file will leave you with the default values for that section.
A notable exception is `binds {}`: they do not get filled with defaults, so make sure you do not erase this section.

### Breaking Change Policy

Configuration backwards compatibility follows the Rust / Cargo semantic versioning standards.
A patch release (i.e. niri 0.1.3 to 0.1.4) must not cause a parse error on a config that worked on the previous version.

A minor release (i.e. niri 0.1.3 to 0.2.0) *can* cause previously valid config files to stop parsing.
When niri reaches 1.0, a major release (i.e. niri 1.0 to 2.0) will be required to break config backwards compatibility.

Exceptions can be made for parsing bugs.
For example, niri used to accept multiple binds to the same key, but this was not intended and did not do anything (the first bind was always used).
A patch release changed niri from silently accepting this to causing a parsing failure.
This is not a blanket rule, I will consider the potential impact of every breaking change like this before deciding to carry on with it.

Keep in mind that the breaking change policy applies only to niri releases.
Commits between releases can and do occasionally break the config as new features are ironed out.
However, I do try to limit these, since several people are running git builds.

[KDL]: https://kdl.dev/
