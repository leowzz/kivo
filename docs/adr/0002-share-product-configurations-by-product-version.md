# Share Product Configurations by Product Version

Product Devices select named Product Configuration Profiles shared only within an exact Product Version ID; those profiles contain trigger settings and actions, while firmware-owned Product Definitions remain the sole source of layout and hardware topology. This keeps same-version production units interchangeable without allowing a configuration created for one immutable hardware/layout contract to run on another, and Product Studio remains the only place that can change that contract.

## Consequences

Editing a Product Configuration Profile affects every Device selecting it. Existing per-device Product Device configs migrate to separate profiles so an upgrade does not silently merge behavior, and legacy Device Profiles and Runtime Assignments remain a distinct path for general-purpose firmware.
