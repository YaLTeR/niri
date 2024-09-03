X11 is very cursed, so built-in Xwayland support is not planned at the moment.
However, there are multiple solutions to running X11 apps in niri.

## Using xwayland-satellite

[xwayland-satellite] is a new project that essentially implements rootless Xwayland in a separate application, without the host compositor's involvement.
While it is still somewhat experimental, it handles a lot of applications correctly, like Steam, games and Discord.

Build it according to instructions from its README, then run the `xwayland-satellite` binary.
Now you can start X11 applications on the X11 DISPLAY that it provides:

```
env DISPLAY=:0 flatpak run com.valvesoftware.Steam
```

They will appear as normal windows.

You can also set `DISPLAY` by default for all apps by adding it to the `environment` section of the niri config:

```kdl
environment {
    DISPLAY ":0"
}
```

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

> [!TIP]
> If you don't run an X11 window manager, Xwayland will close and re-open its window every time all X11 windows close and a new one opens.
> To prevent this, start an X11 WM inside as mentioned above, or open some other long-running X11 window.

One caveat is that currently rootful Xwayland doesn't seem to share clipboard with the compositor.
For textual data you can do it manually using [wl-clipboard](https://github.com/bugaevc/wl-clipboard), for example:

- `env DISPLAY=:0 xsel -ob | wl-copy` to copy from Xwayland to niri clipboard
- `wl-paste -n | env DISPLAY=:0 xsel -ib` to copy from niri to Xwayland clipboard

You can also bind these to hotkeys if you want:

```
binds {
    Mod+Shift+C { spawn "sh" "-c" "env DISPLAY=:0 xsel -ob | wl-copy"; }
    Mod+Shift+V { spawn "sh" "-c" "wl-paste -n | env DISPLAY=:0 xsel -ib"; }
}
```

## Using xwayland-run to run Xwayland

[xwayland-run] is a helper utility to run an X11 client within a dedicated Xwayland rootful server.
It takes care of starting Xwayland, setting the X11 DISPLAY environment variable, setting up xauth and running the specified X11 client using the newly started Xwayland instance.
When the X11 client terminates, xwayland-run will automatically close the dedicated Xwayland server.

It works like this:

```
xwayland-run <Xwayland arguments> -- your-x11-app <X11 app arguments>
```

For example:

```
xwayland-run -geometry 800x600 -fullscreen -- wine wingame.exe
```

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

## Using gamescope

You can use [gamescope](https://github.com/ValveSoftware/gamescope) to run X11 games and even Steam itself.

Similar to Cage, gamescope will only show a single, topmost window, so it's not very suitable to running regular apps.
But you can run Steam in gamescope and then start some game from Steam just fine.

```
gamescope -- flatpak run com.valvesoftware.Steam
```

To run gamescope fullscreen, you can pass flags that set the necessary resolution, and a flag that starts it in fullscreen mode:

```
gamescope -W 2560 -H 1440 -w 2560 -h 1440 -f  -- flatpak run com.valvesoftware.Steam
```

> [!NOTE]
> If Steam terminates abnormally while running in gamescope, it seems that subsequent gamescope invocations will sometimes fail to start it properly.
> If this happens, run Steam inside a rootful Xwayland as described above, then exit it normally, and then you will be able to use gamescope again.

[xwayland-run]: https://gitlab.freedesktop.org/ofourdan/xwayland-run
[xwayland-satellite]: https://github.com/Supreeeme/xwayland-satellite
