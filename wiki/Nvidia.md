Presently, there is a quirk in the NVIDIA drivers that affects niri's VRAM usage (the driver does not properly release VRAM back into the pool). Niri *should* use on the order of 100MiB of VRAM (as checked in [nvtop](https://github.com/Syllo/nvtop); if you see anywhere close to 1GiB of VRAM in use, you are likely seeing this behavior (heap not returning freed buffers to the driver).

Luckily, this can be mitigated through configuring the NVIDIA drivers with a per-process application profile as follows:

* `sudo mkdir -p /etc/nvidia/nvidia-application-profiles-rc.d` to make the config dir if it does not exist (it most likely does not if you are reading this)
* emplace the following json blob to set the `GLVidHeapReuseRatio` config value for the `niri` process:

```
# cat /etc/nvidia/nvidia-application-profiles-rc.d/50-limit-free-buffer-pool-in-wayland-compositors.json
{
    "rules": [
        {
            "pattern": {
                "feature": "procname",
                "matches": "niri"
            },
            "profile": "Limit Free Buffer Pool On Wayland Compositors"
        }
    ],
    "profiles": [
        {
            "name": "Limit Free Buffer Pool On Wayland Compositors",
            "settings": [
                {
                    "key": "GLVidHeapReuseRatio",
                    "value": 1
                }
            ]
        }
    ]
}
```

(The file in `/etc/nvidia/nvidia-application-profiles-rc.d/` can be named anything, and does not actually need an extension).

The application profile should be picked up [by restarting the process](https://download.nvidia.com/XFree86/Linux-x86_64/384.59/README/profiles.html#ApplicationProf9ccbe) after emplacing the application config file.

The upstream issue that this solution was pulled from is [here](github.com/NVIDIA/egl-wayland/issues/126#issuecomment-2379945259). There is a (slim) chance that NVIDIA updates their built-in application profiles to apply this to niri automatically; it is unlikely that the underlying heuristic will see a proper fix.
