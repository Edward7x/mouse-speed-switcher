# Mouse Speed Switcher _(mouse-speed-switcher)_

A Windows tray utility that automatically switches pointer speed and precision settings for each mouse.

Switch between office mice, gaming mice, and other devices without repeatedly opening Windows mouse settings. Moving, clicking, or scrolling an identifiable mouse applies its saved profile.

Written in Rust for **Windows 10/11 x64**, with no administrator privileges, additional drivers, or network service required. Current version: **1.1.8**. The product and project folder use Mouse Speed Switcher; the Cargo package uses the corresponding hyphenated name, `mouse-speed-switcher`.

Repository: [Edward7x/mouse-speed-switcher](https://github.com/Edward7x/mouse-speed-switcher).

**English** | [简体中文](README.zh-CN.md)

## Table of Contents

- [Background](#background)
- [Install](#install)
- [Usage](#usage)
- [Configuration and Recovery](#configuration-and-recovery)
- [How It Works and Limitations](#how-it-works-and-limitations)
- [Development and Building](#development-and-building)
- [Validation](#validation)
- [Contributing](#contributing)
- [License](#license)

## Background

Windows pointer speed and “Enhance pointer precision” are global settings shared by all mice. This utility identifies the device producing an input event and switches to its profile.

- Save a name, pointer speed from 1 to 20, and precision setting for each profile.
- Switch between multiple connected mice, including wireless receivers that remain plugged in. Idle reports do not trigger switching.
- Reassociate devices, pause switching from the tray, enable startup at sign-in, and run a single instance.
- Conditionally restore the original system settings on normal exit without overwriting subsequent changes made by other software.

## Install

### Dependencies

The built EXE requires Windows 10/11 x64. It statically links the VC runtime and does not require Rust. For source build requirements, see [Development and Building](#development-and-building). Compatibility with other Windows machines still requires hardware validation.

### Use the Local Release Package

The current workspace contains `dist/MouseSpeedSwitcher-1.1.8-windows-x64.zip`, including the application, UI preview, English and Chinese READMEs, MIT LICENSE, and validation record. From the project root, run:

```powershell
Expand-Archive -LiteralPath .\dist\MouseSpeedSwitcher-1.1.8-windows-x64.zip -DestinationPath .\MouseSpeedSwitcher-1.1.8
Set-Location .\MouseSpeedSwitcher-1.1.8
.\mouse-speed-switcher.exe
```

You can also run `dist/1.1.8/mouse-speed-switcher.exe` directly. These are local build artifacts; no public download location has been configured.

### Update and Remove

Before updating, select Exit from the old version's tray menu, then launch the new version. The single-instance mechanism activates the existing process rather than replacing it.

If startup at sign-in is enabled, disable and re-enable it after moving or replacing the EXE to update its path.

To remove the application, disable startup, exit from the tray, and delete the application files. Configuration remains in `%LOCALAPPDATA%\MouseSpeedSwitcher`; delete this directory if you no longer need it.

## Usage

The current application UI uses Chinese labels. English explanations below retain the relevant on-screen labels.

### Save Your First Profile

1. Start the application and select a mouse profile in the left pane.
2. Set its name, pointer speed, and “Enhance pointer precision,” then click Save Profile (`保存配置`).
3. Use another mouse and save its preferred settings.
4. Alternate between moving, clicking, or scrolling each mouse and check the active profile in the status area.

Saving the active mouse's profile applies it immediately. Other profiles take effect when their mice are next used or connected. New devices inherit the system settings captured before this application started. At startup, a single identifiable connected mouse loads its profile immediately; with multiple mice, the application waits for input.

### Editing and Device Association

| UI item | Behavior |
| --- | --- |
| Profile list | Connected profiles appear first. ● indicates an associated connected device; ○ indicates a historical profile. |
| Active profile | Identifies the mouse in use, separately from the profile selected for editing. |
| Save Profile (`保存配置`) | Saves the form and applies the settings if this is the active profile. |
| Apply Saved Profile Now (`立即应用已保存配置`) | Applies saved values without saving pending form edits. |
| Associate with Selected Profile (`关联到选中配置`) | Associates the device in the dropdown with the selected profile, retaining the original profile and history. |

Changing the active mouse does not overwrite the editing form. Selecting a different profile loads its saved values, so save edits before changing the selection.

If changing USB ports creates a new profile, select the original profile, choose the newly identified mouse in the device dropdown, and click `关联到选中配置`. Expand the association and identity details as needed.

### Tray and Startup

- Closing the normal settings window leaves the application running in the tray. Double-click its icon to reopen settings.
- Right-click the tray icon to pause or resume switching, configure startup, or exit.
- Pausing stops automatic switching. Applying saved settings or saving the active profile can still change system settings.
- Startup is disabled by default. When enabled, the current user's registry entry `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\MouseSpeedSwitcher` launches the EXE with `--background`.
- Launching the normal application again opens the existing settings window without starting another listener.

### CLI

Run these examples from the directory containing the EXE:

```powershell
# Start normally and show settings
.\mouse-speed-switcher.exe

# Start in the tray
.\mouse-speed-switcher.exe --background

# Preview the UI with sample devices
.\mouse-speed-switcher.exe --preview

# Preview the empty state
.\mouse-speed-switcher.exe --preview --preview-empty

# Read-only diagnostics: write devices and current settings to stdout as JSON
.\mouse-speed-switcher.exe --diagnose
```

Preview mode does not start input monitoring, write user configuration, change system settings, or modify startup entries. Saving, applying, associating, pausing, and startup controls only change in-memory demo state. Closing the preview window exits the preview. A tray icon and context menu are included.

The packaged `mouse-speed-switcher-ui-preview.exe` enters preview mode automatically. Keep the `-preview` filename suffix if relying on filename detection, or pass `--preview` explicitly.

`--diagnose` does not start a listener or create configuration or startup entries. Diagnostic output includes device identity information; redact serial numbers and device paths as needed before sharing.

## Configuration and Recovery

Configuration is stored at:

```text
%LOCALAPPDATA%\MouseSpeedSwitcher\config.json
```

The JSON contains device identities, aliases, speeds, complete acceleration parameters, and device-to-profile associations. It does not record input history or mouse trajectories. The currently supported format is `version: 1`.

Saving uses a temporary file in the same directory, flushes it to disk, and replaces the configuration atomically. The previous valid version is saved as `config.json.bak`. An invalid configuration is first preserved as `config.corrupt-<timestamp>.json`; the application then tries the backup, or uses an empty configuration if recovery fails, and displays a status message. Higher-version configurations are not treated as valid and are preserved before recovery.

Exit the application before manually restoring or editing configuration files.

On normal exit, the application restores the original system settings only if they still match its last successfully applied values. Changes made by other software are retained. Restoration is not guaranteed after forced termination, a crash, or power loss.

## How It Works and Limitations

A background thread uses Windows Raw Input to receive device-specific input and connection events. It does not intercept input, simulate movement, or change hardware DPI. The UI runs independently, and the input hot path accesses cached state. The same profile does not repeatedly write system parameters; files are written only during device discovery or explicit saves. Failed retries are throttled, and errors appear in the status area.

### Device Identification

Identification prefers the Windows Container ID and falls back to the device instance ID. Interface paths, instance information, and available USB serial numbers are also recorded for diagnostics. Devices of the same model are not merged solely by VID/PID.

- Receivers that expose multiple physical mice as one device can only share a profile.
- Devices without stable identifiers may need reassociation after changing ports.
- Devices with unresolved identities, Remote Desktop mice, and input without a source handle do not automatically create profiles.
- Precision touchpads may not provide a device handle; support is not guaranteed.

### System Settings

The application switches **global Windows pointer settings**. The first input triggering a switch may still use the previous settings. When two mice move simultaneously, switching follows event arrival order. Games that consume Raw Input directly may ignore these settings.

Speed uses the native Windows 1–20 scale. Precision maps to the acceleration state exposed by `SPI_GETMOUSE` / `SPI_SETMOUSE`. Saving an enabled state preserves the complete parameters; switching from disabled to enabled uses `[6, 10, 1]`. Disabling preserves thresholds and sets the acceleration state to 0. Switching does not repeatedly persist Windows registry parameters. Advanced custom acceleration curves are not currently configurable.

## Development and Building

### Build Dependencies

- Rust stable with the `x86_64-pc-windows-msvc` toolchain and Rust 2024 edition support.
- Visual Studio Build Tools with Desktop development with C++, including the Windows SDK.

From the project root in Developer PowerShell, run:

```powershell
cargo fetch --locked
cargo fmt -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

Once dependencies are cached, you can use the offline build script:

```powershell
.\build.ps1 -Task All
```

`All` runs formatting checks, Clippy, tests, and the release build. Individual tasks are `Check`, `Test`, and `Build`. Output is `target/release/mouse-speed-switcher.exe`; release builds do not show a console window.

### Optional Local SDK

The build script also supports Microsoft CRT/SDK files in `target/tooling/winsysroot`, using Rust's bundled `rust-lld`. This directory is a build dependency and is not distributed with the EXE.

Local validation used [xwin](https://github.com/Jake-Shadle/xwin) 0.10.0 to prepare this directory. To use this approach, obtain xwin and verify its SHA256, read and accept the Microsoft SDK license, then run:

```powershell
xwin --accept-license --cache-dir target/tooling/xwin-cache splat --disable-symlinks --use-winsysroot-style --preserve-ms-arch-notation --output target/tooling/winsysroot
.\build.ps1 -Task All
```

Missing Microsoft CRT debug symbols may produce LNK4099 warnings during debug or test linking. These warnings did not cause the recorded builds to fail; see the [validation record](VALIDATION.md), currently available in Chinese.

### UI and Icons

The UI uses a light two-column layout, scrollable content, and persistent bottom actions. Local designs and functional specifications are in `design/ui-v1/`, with implementation notes in `design/ui-v1/IMPLEMENTATION.md`. These design materials are excluded from Git.

The icon depicts two blue and purple mice. Its vector source is [assets/app-icon.svg](assets/app-icon.svg). The ICO includes nine sizes from 16 to 256px for the EXE, settings window, and tray. `build.rs` embeds resources without an additional resource compiler.

When changing the vector design, also update the geometry in `scripts/export-icon.py`. With Python and Pillow installed, run:

```powershell
python -m pip install Pillow
python scripts/export-icon.py
```

## Validation

Automated tests use mocked system parameter interfaces and hidden native controls. They cover input deduplication, write failures, pausing and explicit application, exit restoration, device identity, profile association and recovery, and UI and tray regressions. They do not change real mouse settings.

[VALIDATION.md](VALIDATION.md), currently in Chinese, records passing formatting checks, Clippy, 17 tests, and a release build for 1.1.8, along with historical results and outstanding checks. It does not establish that every hardware scenario has passed real-device validation.

Hardware validation should cover:

1. Alternating movement, clicks, and scrolling between two mice, checking actual Windows speed and precision settings.
2. USB reconnection, Bluetooth reconnection, continuously plugged-in receivers, multiple devices at startup, and removal of the last device.
3. Port changes, distinct devices of the same model, reassociation, and profile persistence after restart.
4. Active mouse changes while editing, tray pause, repeated launches, and startup at sign-in.
5. Restoring settings on normal exit and preserving changes made by other software.
6. High-polling-rate mouse response and resource usage, different display DPI settings, and compatibility across Windows machines.

## Contributing

Collaborators with repository access can report problems and suggestions through [Issues](https://github.com/Edward7x/mouse-speed-switcher/issues) and submit improvements through [Pull requests](https://github.com/Edward7x/mouse-speed-switcher/pulls). Without repository access, use the existing project collaboration channel.

Include your Windows version, application version, device and connection type, reproduction steps, and expected behavior. For identification problems, include redacted `--diagnose` output when useful.

Before submitting code changes, run `build.ps1 -Task All`. Describe hardware validation results for changes involving device switching, system settings, the tray, or startup. Passing builds and automated tests do not replace hardware validation.

## License

Licensed under the [MIT License](LICENSE). Copyright (c) 2026 Eddie7x.

Use, modification, distribution, and commercial use are permitted, subject to retaining the copyright and license notices. See the license file for full terms and disclaimers. Third-party dependencies retain their respective licenses.

## Interface language

The settings window, tray menu, status messages and application errors support English and Simplified Chinese. The app follows the Windows display language: Chinese uses Simplified Chinese; other languages use English. Device names and profile names are preserved.

Use **中文 / English** at the bottom of the settings window to switch immediately, or select **Chinese** / **English** under **Language** in the tray menu. Your choice is saved for future launches and automatic startup. Switching preserves unsaved profile edits. Preview mode changes language in memory only.

Optional launch override (takes precedence over the saved choice):

```powershell
.\mouse-speed-switcher.exe --lang en
.\mouse-speed-switcher.exe --lang zh-CN
.\mouse-speed-switcher.exe --preview --lang en
```

To use a launch override, exit the running app from the tray first. The command-line override alone is not saved. Preferences are stored in `%LOCALAPPDATA%\MouseSpeedSwitcher\language.txt`.
