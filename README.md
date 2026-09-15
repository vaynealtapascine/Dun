# Dun

Reminders and countdown timers that keep nagging, every minute by default, until you mark them done. Inspired by Due for iOS.

- Runs in the Windows tray and starts at login. Timers and reminders survive reboots.
- Snooze from the notification (Done · +1m · +5m · +15m on Windows).
- An Android build shares the same list with the PC over your LAN or Tailscale, with no cloud. The PC rings while you're at it; otherwise the phone does.

> Status: early development. See `docs/` for what is implemented and verified so far.

## Development setup (Windows)

### Prerequisites
- Node 22.12+ and npm
- Rust stable (`rust-toolchain.toml` pins the channel and pulls in the Android targets)
- Visual Studio Build Tools with the C++ workload, and WebView2 (preinstalled on Windows 11)
- For Android:
  - Android SDK with `platforms;android-36` and build-tools 36
  - NDK r28+
  - A JDK 17+ (Android Studio's bundled `jbr` works)

### Build output location
Rust and Gradle build output runs to tens of GB. To keep it off a small system drive, create a machine-local, git-ignored `.cargo/config.toml`:

```toml
[build]
target-dir = "F:/DunBuild/target"
```

`scripts/dev-env.ps1` uses the same root (`DUN_BUILD_ROOT`, default `F:\DunBuild`) for the NDK (`android-sdk\ndk\<version>`) and `GRADLE_USER_HOME`.

### Common commands
```powershell
npm install
npm run tauri dev                 # desktop app with hot reload
npm test                          # vitest
cargo test --workspace            # Rust tests

. .\scripts\dev-env.ps1           # per-session JAVA_HOME / ANDROID_HOME / NDK_HOME
npx tauri android dev             # run on a connected device or emulator
npx tauri android build --debug --apk --target aarch64

pwsh scripts/verify.ps1 -Quick    # all checks without bundling
pwsh scripts/verify.ps1           # plus the NSIS installer
```

## Layout
| Path | What |
|---|---|
| `src/` | Svelte 5 UI, shared by desktop and Android |
| `crates/dun-core/` | Platform-neutral domain model, recurrence, scheduler, sync merge, SQLite storage |
| `src-tauri/` | Tauri app shell (desktop and Android), commands, Windows integration |
| `src-tauri/gen/android/` | Generated Android project, committed. Custom Kotlin lives in the local plugin, not here |
| `scripts/` | `dev-env.ps1`, `verify.ps1` |
