# Kivo Rename Design

## Goal

Rename the packaged application from `Vibe Tool` to `Kivo` so its Windows
installation directory contains no product-name space and the product has a
less literal brand.

## Naming

- User-visible product name: `Kivo`
- Package, crate, and executable name: `kivo`
- Application identifier: `cn.wleo.kivo`
- Firmware USB product name: `Kivo Keyboard`

Apply the user-visible name to the installer, application window, HTML title,
main heading, tray menu, tray tooltip, and runtime error text. Rename internal
package metadata and temporary test-file prefixes where they still use the old
name.

## Configuration

Changing the application identifier creates a new platform configuration
directory. Do not migrate or read `com.leose.vibetool`; existing configuration
has been backed up by the user.

## Verification

Search tracked source and packaging files for remaining `Vibe Tool`,
`vibe-tool`, `vibetool`, and `com.leose.vibetool` references. Run the existing
frontend, Rust, firmware, and packaging checks relevant to the changed files.
