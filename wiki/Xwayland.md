X11 is very cursed, so built-in Xwayland support is not planned at the moment.
However, there are multiple solutions to running X11 apps in niri.

## Directly running Xwayland in rootful mode
This method involves invoking XWayland directly and running it as its own window, it also requires an extra X11 window manager running inside it.

![Xwayland running in rootful mode.](https://github.com/YaLTeR/niri/assets/1794388/b64e96c4-a0bb-4316-94a0-ff445d4c7da7)

Here's how you do it:

1. Run `Xwayland` (just the binary on its own without flags).
This will spawn a black window which you can resize and fullscreen (with Mod+Shift+F) for convenience.
On older Xwayland versions the window will be screen-sized and non-resizable.
1. Run some X11 window manager in there, e.g. `env DISPLAY=:0 i3`.
This way you can manage X11 windows inside the Xwayland instance.
1. Run an X11 application there, e.g. `env DISPLAY=:0 flatpak run com.valvesoftware.Steam`.

With fullscreen game inside a fullscreen Xwayland you get pretty much a normal gaming experience.

One caveat is that currently rootful Xwayland doesn't seem to share clipboard with the compositor.
For textual data you can do it manually using [wl-clipboard](https://github.com/bugaevc/wl-clipboard), for example:

- `env DISPLAY=:0 xsel -ob | wl-copy` to copy from Xwayland to niri clipboard
- `wl-paste | env DISPLAY=:0 xsel -ib` to copy from niri to Xwayland clipboard

## Using the Cage Wayland compositor

It is also possible to run the X11 application in [Cage](https://github.com/cage-kiosk/cage), which runs a nested Wayland session which also supports Xwayland, where the X11 application can run in.

Compared to the Xwayland rootful method, this does not require running an extra X11 window manager, and can be used with one command `cage -- /path/to/application`. However, it can also cause issues if multiple windows are launched inside Cage, since Cage is meant to be used in kiosks, every new window will be automatically full-screened and take over the previously opened window.

To use Cage you need to:

1. Install `cage`, it should be in most repositories.
2. Run `cage -- /path/to/application` and enjoy your X11 program on niri.

Optionally one can also modify the desktop entry for the application and add the `cage --` prefix to the `Exec` property. The Spotify Flatpak for example would look something like this:

```ini
[Desktop Entry]
Type=Application
Name=Spotify
GenericName=Online music streaming service
Comment=Access all of your favorite music
Icon=com.spotify.Client
Exec=cage -- flatpak run com.spotify.Client
Terminal=false
```