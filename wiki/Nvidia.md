### High VRAM usage fix

Presently, there is a quirk in the NVIDIA drivers that affects niri's VRAM usage (the driver does not properly release VRAM back into the pool). Niri *should* use on the order of 100 MiB of VRAM (as checked in [nvtop](https://github.com/Syllo/nvtop)); if you see anywhere close to 1 GiB of VRAM in use, you are likely hitting this issue (heap not returning freed buffers to the driver).

Luckily, you can mitigate this by configuring the NVIDIA drivers with a per-process application profile as follows:

* `sudo mkdir -p /etc/nvidia/nvidia-application-profiles-rc.d` to make the config dir if it does not exist (it most likely does not if you are reading this)
* write the following JSON blob to set the `GLVidHeapReuseRatio` config value for the `niri` process into the file `/etc/nvidia/nvidia-application-profiles-rc.d/50-limit-free-buffer-pool-in-wayland-compositors.json`:
    
    ```json
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
                        "value": 0
                    }
                ]
            }
        ]
    }
    ```
    
    (The file in `/etc/nvidia/nvidia-application-profiles-rc.d/` can be named anything, and does not actually need an extension).

The change should be picked up [by restarting niri](https://download.nvidia.com/XFree86/Linux-x86_64/384.59/README/profiles.html#ApplicationProf9ccbe) after writing the config file.

The upstream issue that this solution was pulled from is [here](https://github.com/NVIDIA/egl-wayland/issues/126#issuecomment-2379945259). There is a (slim) chance that NVIDIA updates their built-in application profiles to apply this to niri automatically; it is unlikely that the underlying heuristic will see a proper fix.

The fix shipped in the driver at the time of writing uses a value of 0, while the initial config posted by an Nvidia engineer approximately a year prior used a value of 1. 
