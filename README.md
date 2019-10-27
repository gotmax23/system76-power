[![COPR](https://copr.fedorainfracloud.org/coprs/szydell/system76/package/system76-power/status_image/last_build.png)](https://copr.fedorainfracloud.org/coprs/szydell/system76/package/system76-power/)

# system76-power
System76 Power Management

Repository for automatic RPM creation.

Repo is available here: https://copr.fedorainfracloud.org/coprs/szydell/system76/

# enchancements
+ bash completion 
=======
# System76 Power Management

**system76-power** is a utility for managing graphics and power profiles.

## Graphics

### Intel

The iGPU (Intel) is used exclusively.

Lower graphical performance with a longer battery life.

External displays connected to the dGPU ports cannot be used.

### NVIDIA

The dGPU (NVIDIA) is used exclusively.

Higher graphical performance at the expense of a shorter battery life.

Allows using external displays.

### Hybrid

Enables PRIME render offloading. The iGPU is used as the primary renderer, with
the ability to have specific applications render using the dGPU.

PRIME render offloading requires the 435.17 NVIDIA drivers or later.

Applications must use [GLVND] to take advantage of this feature, so may not
render on the dGPU even when requested. Vulkan applications must be launched
with `__NV_PRIME_RENDER_OFFLOAD=1` to render on the dGPU. GLX applications must
be launched with `__NV_PRIME_RENDER_OFFLOAD=1 __GLX_VENDOR_LIBRARY_NAME=nvidia`
to render on the dGPU.

External displays connected to the dGPU ports cannot be used. The NVIDIA
drivers currently do not support display offload sink ("reverse PRIME") when
configured for render offloading.

NVIDIA driver power management is only fully implemented for Turing cards. This
allows them to enter a low power state when not used. It currently requires
manually adding some [udev rules] to work correctly. Pascal cards are not
supported and will remain on, even when not in use.


[GLVND]: https://github.com/NVIDIA/libglvnd
[udev rules]: https://download.nvidia.com/XFree86/Linux-x86_64/435.21/README/dynamicpowermanagement.html#AutomatedSetup803b0
