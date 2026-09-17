# Releasing

Two artifacts: a Windows installer and an Android APK. Both come out of
`scripts/verify.ps1`, which refuses to build either until every check passes.

```powershell
pwsh scripts/verify.ps1            # checks + the NSIS installer
pwsh scripts/verify.ps1 -Android   # also the APK, and checks it
```

Bump `version` in `src-tauri/tauri.conf.json` first; the installer, the app's
About line and the Android `versionName` all read it.

## Windows

```powershell
npx tauri build --bundles nsis
```

The installer lands in `F:\DunBuild\target\release\bundle\nsis\` (or wherever
`.cargo/config.toml` points `target-dir`). It installs per-user, so no
administrator prompt.

`src-tauri/windows/hooks.nsh` runs on uninstall and removes the three things
Dun keeps outside its own folder: the toast `AppUserModelId`, the COM
activator's `CLSID`, and the login-autostart entry. If the toast identity or
the product name ever changes, change them there too.

The sync firewall rule is not removed by the uninstaller, which runs as the
current user without elevation. It allows an exe that no longer exists; to
clear it out anyway, from an elevated prompt:

```powershell
netsh advfirewall firewall delete rule name="Dun Sync (TCP-In)"
```

## Android

Release builds are signed with a key that is **not** in this repository, and
must not be: anyone holding it can publish an update that Android will install
over yours.

Create one once, keeping it outside the repo:

```powershell
keytool -genkeypair -v -keystore $HOME\dun-release.jks -alias dun `
  -keyalg RSA -keysize 4096 -validity 10000
```

Then write `src-tauri/gen/android/keystore.properties` (git-ignored):

```properties
storeFile=C:/Users/you/dun-release.jks
storePassword=…
keyAlias=dun
keyPassword=…
```

Back up the `.jks` and its passwords somewhere you won't lose them. A lost key
means the next release can only be installed by uninstalling the old one first,
which takes its data with it.

```powershell
. .\scripts\dev-env.ps1
npx tauri android build --apk --target aarch64
```

Without `keystore.properties` the build still runs and produces an **unsigned**
APK, which Android will refuse to install. That is deliberate: a missing key
should be obvious, not silently replaced by a debug signature.

The verify script checks two things on the built APK that only show up at
runtime otherwise:

- `llvm-nm -D` finds `Java_app_dun_android_DunNative_handleEvent`. R8 renames
  `external` methods without the keep rules in `consumer-rules.pro`, and the
  first sign of that is an alarm that does nothing.
- `zipalign -c -P 16` passes: Android 15 and later require 16 KB page
  alignment.

## Sending a build to the phone

Once a phone is paired it can fetch new builds from the PC instead of a cable.
Drop the APK into the PC's app data folder, named for its version:

```powershell
mkdir -Force "$env:APPDATA\app.dun\updates"
copy .\src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release.apk `
     "$env:APPDATA\app.dun\updates\dun-0.3.0.apk"
```

The name has to be `dun-<version>.apk`; anything else is ignored, and the
highest version in the folder is the one offered. Dun reads the file only when
it changes, so replacing it is enough — no restart.

A paired phone running something older then sees it in **Settings -> Sync with
your PC**, with the size, and fetches it over the same pinned connection as
everything else (so Tailscale works too). The hash is checked on arrival, and
the file is handed to Android's own installer, which asks before replacing
anything. The first time, Android also wants "install unknown apps" allowed for
Dun.

Nothing downloads or installs on its own, and a phone already on that version
is never offered it.

## Installing

Windows: run the installer; Dun starts at login and lives in the tray.

Android: `adb install -r app-universal-release.apk`, or copy the APK to the
phone and open it. Then work through **Settings → Ringing reliably** on the
phone, which is what actually decides whether reminders arrive — see
[HELP.md](HELP.md).
