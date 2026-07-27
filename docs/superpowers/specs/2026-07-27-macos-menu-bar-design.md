# macOS Menu Bar Design

## Goal

Run Vibe Tool as a macOS menu-bar application without a Dock icon. The menu bar
must provide a quick connection-status view, reopen the existing main window,
and quit the application cleanly.

## Scope

This feature is macOS-only and uses Tauri 2's native tray and menu APIs. It does
not add a custom popover, a second window, startup-at-login behavior, or a new
dependency. The React application and serial protocol remain unchanged.

## Menu Behavior

The menu bar uses a monochrome template icon. Clicking it opens a native menu
with these items in order:

1. A disabled status item showing `Connected - <port>` or `Waiting for device`.
2. A separator.
3. `Open Vibe Tool`, which shows and focuses the existing `main` window.
4. `Quit Vibe Tool`, which exits through the existing Tauri lifecycle.

Connection changes update the status item and tray tooltip from the same
`ConnectionStatus` value used by the frontend. The menu does not maintain a
second connection state.

## Application Lifecycle

During Tauri setup, macOS activation policy is set to `Accessory`, so Vibe Tool
does not appear in the Dock. Setup builds the native menu and tray icon before
starting the serial worker.

Closing the main window continues to hide it while the worker remains active.
The tray's open action restores and focuses that same window. The quit action
calls `AppHandle::exit(0)`; the existing exit handler sets the stop flag and
joins the single serial worker before process termination.

## Structure

Tray creation, status formatting, and menu-action handling live in a focused
`src-tauri/src/tray.rs` module. `device.rs` calls one tray-status update function
when its authoritative connection value changes. `lib.rs` only initializes the
tray and preserves the existing application lifecycle.

The template PNG lives under `src-tauri/icons` and is derived from the already
installed Lucide USB icon, so no icon or UI dependency is added.

## Error Handling

Failure to create the native menu or tray fails Tauri setup with its original
error instead of silently running an inaccessible Dock-less application.
Failure to show or focus the main window is ignored because the window may be
closing during shutdown. Status-update failures do not stop serial processing.

## Verification

Rust tests cover connection-status labels and menu-action routing. Existing
firmware, Rust, and React suites must remain green; Clippy and the production
macOS bundle must pass.

A packaged-app smoke test must prove all of the following:

- the process uses macOS `Accessory` activation policy and has no Dock icon;
- the menu-bar item is visible with a native status menu;
- connected and waiting states update the menu status;
- `Open Vibe Tool` restores and focuses the hidden main window;
- `Quit Vibe Tool` terminates the process and releases the serial port;
- relaunching the packaged application recreates the tray and reconnects.
