# Shared Full-Height Layout Design

## Goal

Make the home, button behavior, hardware mapping, button layout, and empty-state pages fill the Kivo window through one shared layout contract. Keep the existing visual design and dependencies.

## Root Cause

The application shell relies on viewport height while its child pages use different sizing models: fixed viewport calculations, percentage heights, minimum heights, and content height. After a Tauri window resize or scale change, the shell can stop above the visible window edge, and individual pages do not consistently consume the remaining workspace.

## Design

Use one height chain from `html`, `body`, and `#root` through `product-shell`, `product-workspace`, and `content-panel`.

The shell height will come from a CSS custom property containing the Tauri window's logical inner height. It will update after window resize and scale changes. Browser preview mode and failed Tauri size queries will fall back to `100dvh`.

`content-panel` will be the shared column container. Each page root will use the same `flex: 1` and `min-height: 0` contract. Scrolling will remain at leaf content regions such as the keypad stage, hardware source list, action list, and activity log, so the sidebar and page frame stay full height.

Responsive breakpoints will keep their current stacked behavior. No UI component library, new shared component, visual redesign, or business-logic change is included.

## Error Handling

Failure to read the Tauri window size must not block rendering. The CSS `100dvh` fallback remains active, and resize listeners are removed when the application unmounts.

## Verification

1. Add a focused frontend test for converting the Tauri physical window height to CSS logical pixels.
2. Verify resize and scale-change subscriptions update the shared shell height and are cleaned up.
3. Run the complete frontend test suite and production build.
4. Inspect all four navigation pages and the no-model empty state at the default 1120 x 760 window and after enlarging the window. The shell, sidebar, main content, and optional right panel must reach the bottom edge without page-level overflow or blank space.
