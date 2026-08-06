# Windows Compatibility Design

## Scope

Support Windows 10/11 x64 with the existing GPIO, hotkey, and paste behavior,
and make a normal Windows Tauri build produce an NSIS installer. Code signing, Windows 7,
offline WebView2 packaging, and automated publishing are out of scope.

## Design

Keep the existing serial and USB HID flow. Hotkey actions already use
platform-neutral HID modifier masks and need no change.

For paste actions, the Tauri helper writes text to the host clipboard with
`/usr/bin/pbcopy` on macOS and the Win32 Unicode clipboard API on Windows. After a
successful copy, macOS keeps the existing `PASTE` response, while Windows uses
the existing `HOTKEY` response to request `Control+Shift+V`. Firmware continues to
accept both response forms.

Add `src-tauri/tauri.windows.conf.json` with an NSIS bundle target. This keeps the
current macOS app bundle configuration unchanged while making `tauri build` on
Windows produce an NSIS installer by default.

Windows uses the same native tray menu as macOS, without macOS template-icon or
activation-policy behavior. Closing the main window keeps the helper available
from the notification area. File replacement uses `MoveFileExW` so repeated
profile and settings saves retain atomic replacement semantics on Windows.

RP2040 discovery combines COM-port observations with a targeted Windows PnP
query. Runtime and BOOTSEL identities are reconciled by stable USB location, and
picotool receives the selected BOOTSEL serial number so multiple connected boards
cannot be confused.

## Errors And Verification

Clipboard process failures remain visible as runtime `SKIP` errors, so a failed
copy cannot paste stale clipboard contents. Unit tests cover the platform-aware
paste response. Verification runs the firmware tests, Rust tests, frontend
tests, frontend build, JSON validation, and diff checks. Windows CI runs these
checks and builds the NSIS installer on `windows-latest`.
