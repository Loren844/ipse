# Install Ipse

Ipse is a desktop app for macOS, Windows and Linux. Download it from the [latest release](https://github.com/Loren844/ipse/releases/latest): under **Assets**, the file for your system.

| System | File |
| --- | --- |
| Mac with an Apple chip (M1 and later) | `Ipse-<version>-macos-apple-silicon.dmg` |
| Mac with an Intel chip | `Ipse-<version>-macos-intel.dmg` |
| Windows 10 or 11 | `Ipse-<version>-windows-x64.msi` |
| Linux (Debian, Ubuntu) | `Ipse-<version>-linux-x64.deb` |
| Linux (others) | `Ipse-<version>-linux-x64.AppImage` |

The app comes with the `ipse` command, which your AI apps start to read your memory: **Connect an AI** in the app sets them up.

## macOS

1. Open the `.dmg` and drag **Ipse** to **Applications**.
2. Open Ipse from Applications. The first time, macOS may say it cannot check Ipse for malicious software: Ipse is not yet signed with an Apple Developer certificate.
3. Open **System Settings → Privacy & Security**, scroll to the message about Ipse, and click **Open Anyway**, then confirm. macOS remembers it: the next times, Ipse opens directly.

## Windows

1. Open the `.msi`. If Windows says **Windows protected your PC**, click **More info**, then **Run anyway**: the installer is not yet signed with a code signing certificate.
2. Follow the installer; Ipse is then in the Start menu.

## Linux

- **Debian, Ubuntu**: `sudo apt install ./Ipse-<version>-linux-x64.deb`. Ipse is then among your applications, and `ipse` is on your path.
- **Other distributions**: make the AppImage executable (`chmod +x Ipse-*.AppImage`) and open it.

## The first time

Ipse asks what to open: **create your memory** (a new folder in your name), **try it with an invented memory**, or **open an existing memory** made earlier with `ipse init`. It opens the same memory at each start; **Settings → Open or create another memory** goes back to that choice. Then [import your contacts and calendar](getting-started.md#3-import-your-contacts-and-your-calendar) from **Sources**, and connect your AI.

Closing the window does not quit Ipse: it stays in the menu bar (macOS) or the tray (Windows, Linux), near the clock, and keeps the calendars you follow up to date ([Keep your calendar in sync](calendar.md)). Its icon's menu opens the window again, syncs now, or quits Ipse. On Linux, the tray needs `libayatana-appindicator3-1`, which the `.deb` installs; on a desktop without a tray, closing the window quits Ipse.

## New versions

At its first start, Ipse asks whether to check for new versions; **Settings → Check for updates** changes it later. When it checks, it asks GitHub once at start and once a day, and nothing else ([threat model](threat-model.md)). A new version shows at the top of the window with what changes: **Install and restart** downloads it, checks its signature, installs it and opens it again; **Later** hides it until the next check. If a new version changes the format of your memory's files, Ipse says so before you install it. On Windows the installer shows its progress; on Linux, an app installed from the `.deb` asks for your password to update.

## Uninstall

- **macOS**: drag Ipse from Applications to the Bin.
- **Windows**: Settings → Apps → Ipse → Uninstall.
- **Linux**: `sudo apt remove ipse`, or delete the AppImage.

Your memory stays where it is: it is a folder of your files, which uninstalling never touches. Delete it yourself if you no longer want it.

## Without the app

The `ipse` command alone, for servers or the terminal, installs with one line: see [Getting started](getting-started.md).
