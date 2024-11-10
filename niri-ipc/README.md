# niri-ipc

Types and helpers for interfacing with the [niri](https://github.com/YaLTeR/niri) Wayland compositor.

## Backwards compatibility

This crate follows the niri version.
It is **not** API-stable in terms of the Rust semver.
In particular, expect new struct fields and enum variants to be added in patch version bumps.

Use an exact version requirement to avoid breaking changes:

```toml
[dependencies]
niri-ipc = "=0.1.10"
```
