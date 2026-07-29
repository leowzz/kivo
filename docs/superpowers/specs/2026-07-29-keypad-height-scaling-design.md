# Keypad Height Scaling Design

## Goal

Keep every keypad group visible in the default 1120 x 760 desktop window without scaling the sidebar or action editor. Very small windows may scroll rather than making keys unusably small.

## Root Cause

The keypad is constrained only by width. Each key uses a fixed aspect ratio, so wider content produces a keypad taller than the available workspace and pushes the bottom group below the viewport.

## Design

Keep the existing group structure and CSS grid. `Keypad` will calculate each group's row count from its button count and column count, then expose both the row count and `rows / columns` height weight through inline CSS properties.

The keypad container will use the available stage height. Groups will share that height according to their weights, and each group will divide its share evenly across its rows. Keys will retain the current minimum usable height; layouts that cannot fit at that minimum will continue to scroll.

This changes only `src/Keypad.tsx` and `src/App.css`. It adds no resize observer, component state, or dependency.

## Verification

1. Add a focused component test that checks generated row counts and height weights for groups with different column counts.
2. Run the test once before implementation to confirm the missing behavior fails, then again after implementation.
3. Run the full frontend test suite and production build.
4. Inspect the preview at 1120 x 760 and the configured 760 x 560 minimum size. At the default size all three groups must be visible without page overflow; at minimum size scrolling is acceptable and keys remain usable.
