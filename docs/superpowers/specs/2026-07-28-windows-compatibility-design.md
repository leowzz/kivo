# Windows Compatibility Design

## Scope

Support Windows 10/11 x64 with the existing GPIO, hotkey, and paste behavior,
and make a normal Windows Tauri build produce an MSI. Code signing, Windows 7,
offline WebView2 packaging, and automated publishing are out of scope.

## Design

Keep the existing serial and USB HID flow. Hotkey actions already use
platform-neutral HID modifier masks and need no change.

For paste actions, the Tauri helper writes text to the host clipboard with the
native command: `/usr/bin/pbcopy` on macOS and `clip.exe` on Windows. After a
successful copy, it reuses the existing `HOTKEY` device response to request
`Command+V` on macOS or `Control+V` on Windows. The legacy `PASTE` response
remains accepted by firmware, but the helper no longer needs to emit it.

Add `src-tauri/tauri.windows.conf.json` with an MSI bundle target. This keeps the
current macOS app bundle configuration unchanged while making `tauri build` on
Windows produce an MSI by default.

## Errors And Verification

Clipboard process failures remain visible as runtime `SKIP` errors, so a failed
copy cannot paste stale clipboard contents. Unit tests cover the platform-aware
paste response. Verification runs the firmware tests, Rust tests, frontend
tests, frontend build, JSON validation, and diff checks. Windows MSI generation
must ultimately run on Windows because WiX cannot create MSI files on macOS.
