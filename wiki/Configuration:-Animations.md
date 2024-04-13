### Overview

Niri has several animations which you can configure in the same way.
Additionally, you can disable or slow down all animations at once.

Here's a quick glance at the available animations with their default values.

```
animations {
    // Uncomment to turn off all animations.
    // You can also put "off" into each individual animation to disable it.
    // off

    // Slow down all animations by this factor. Values below 1 speed them up instead.
    // slowdown 3.0

    // Individual animations.

    workspace-switch {
        spring damping-ratio=1.0 stiffness=1000 epsilon=0.0001
    }

    horizontal-view-movement {
        spring damping-ratio=1.0 stiffness=800 epsilon=0.0001
    }

    window-movement {
        spring damping-ratio=1.0 stiffness=800 epsilon=0.0001
    }

    window-open {
        duration-ms 150
        curve "ease-out-expo"
    }

    window-close {
        duration-ms 150
        curve "ease-out-quad"
    }

    window-resize {
        spring damping-ratio=1.0 stiffness=800 epsilon=0.0001
    }

    config-notification-open-close {
        spring damping-ratio=0.6 stiffness=1000 epsilon=0.001
    }
}
```

### Animation Types

There are two animation types: easing and spring.
Each animation can be either an easing or a spring.

#### Easing

This is a relatively common animation type that changes the value over a set duration using an interpolation curve.

To use this animation, set the following parameters:

- `duration-ms`: duration of the animation in milliseconds.
- `curve`: the easing curve to use.

```
animations {
    window-open {
        duration-ms 150
        curve "ease-out-expo"
    }
}
```

Currently, niri only supports three curves:

- `ease-out-quad` <sup>Since: 0.1.5</sup>
- `ease-out-cubic`
- `ease-out-expo`

You can get a feel for them on pages like [easings.net](https://easings.net/).

#### Spring

Spring animations use a model of a physical spring to animate the value.
They notably feel better with touchpad gestures, because they take into account the velocity of your fingers as you release the swipe.
Springs can also oscillate / bounce at the end with the right parameters if you like that sort of thing, but they don't have to (and by default they mostly don't).

Due to springs using a physical model, the animation parameters are less obvious and generally should be tuned with trial and error.
Notably, you cannot directly set the duration.
You can use the [Elastic](https://flathub.org/apps/app.drey.Elastic) app to help visualize how the spring parameters change the animation.

A spring animation is configured like this, with three mandatory parameters:

```
animations {
    workspace-switch {
        spring damping-ratio=1.0 stiffness=1000 epsilon=0.0001
    }
}
```

The `damping-ratio` goes from 0.1 to 10.0 and has the following properties:

- below 1.0: underdamped spring, will oscillate in the end.
- above 1.0: overdamped spring, won't oscillate.
- 1.0: critically damped spring, comes to rest in minimum possible time without oscillations.

However, even with damping ratio = 1.0, the spring animation may oscillate if "launched" with enough velocity from a touchpad swipe.

Lower `stiffness` will result in a slower animation more prone to oscillation.

Set `epsilon` to a lower value if the animation "jumps" at the end.

> [!TIP]
> The spring *mass* (which you can see in Elastic) is hardcoded to 1.0 and cannot be changed.
> Instead, change `stiffness` proportionally.
> E.g. increasing mass by 2× is the same as decreasing stiffness by 2×.

### Animations

Now let's go into more detail on the animations that you can configure.

#### `workspace-switch`

Animation when switching workspaces up and down, including after the vertical touchpad gesture (a spring is recommended).

```
animations {
    workspace-switch {
        spring damping-ratio=1.0 stiffness=1000 epsilon=0.0001
    }
}
```

#### `horizontal-view-movement`

All horizontal camera view movement animations, such as:

- When a window off-screen is focused and the camera scrolls to it.
- When a new window appears off-screen and the camera scrolls to it.
- When a window resizes bigger and the camera scrolls to show it in full.
- After a horizontal touchpad gesture (a spring is recommended).

```
animations {
    horizontal-view-movement {
        spring damping-ratio=1.0 stiffness=800 epsilon=0.0001
    }
}
```

#### `window-movement`

<sup>Since: 0.1.5</sup>

Window movement animations, currently cover only horizontal column movement.

This animation runs on actions like `move-column-left` and `move-column-right` to move the windows themselves.
It can sometimes run together with the `horizontal-view-movement` animation, if the camera also moves.

Since 0.1.5, this is also the animation that moves windows out of the way upon window opening and closing.

```
animations {
    window-movement {
        spring damping-ratio=1.0 stiffness=800 epsilon=0.0001
    }
}
```

#### `window-open`

Window opening animation.

This one uses an easing type by default.

```
animations {
    window-open {
        duration-ms 150
        curve "ease-out-expo"
    }
}
```

#### `window-close`

<sup>Since: 0.1.5</sup>

Window closing animation.

This one uses an easing type by default.

```
animations {
    window-open {
        duration-ms 150
        curve "ease-out-quad"
    }
}
```

#### `window-resize`

<sup>Since: 0.1.5</sup>

Window resize animation.

Only manual window resizes are animated, i.e. when you resize the window with `switch-preset-column-width` or `maximize-column`.
Also, very small resizes (up to 10 pixels) are not animated.

```
animations {
    window-resize {
        spring damping-ratio=1.0 stiffness=800 epsilon=0.0001
    }
}
```

#### `config-notification-open-close`

The open/close animation of the config parse error and new default config notifications.

This one uses an underdamped spring by default (`damping-ratio=0.6`) which causes a slight oscillation in the end.

```
animations {
    config-notification-open-close {
        spring damping-ratio=0.6 stiffness=1000 epsilon=0.001
    }
}
```
