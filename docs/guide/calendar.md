# Keep your calendar in sync

[Getting started](getting-started.md) imports a calendar file once. To follow your calendar as it changes, subscribe to its link instead.

Copy your calendar's link in iCal format:

- **Google Calendar:** on [calendar.google.com](https://calendar.google.com), **Settings**, pick your calendar under "Settings for my calendars", then **Integrate calendar → Secret address in iCal format**.
- **Outlook:** **Settings → Calendar → Shared calendars → Publish a calendar**, choose "Can view all details", then copy the ICS link.
- **iCloud:** in the Calendar app, the calendar's menu → **Share Calendar → Public Calendar**. Apple makes the calendar readable by anyone who has this link.

Whoever has the link can read the calendar: Ipse keeps it in your system's keychain, never in your memory's files. Then:

```sh
ipse calendar subscribe "<the link>" --name Perso --vault ~/Ipse
```

Each event becomes a file in `events/`, its guests linked to your contacts. `ipse sync --vault ~/Ipse` syncs your calendars again; `ipse calendar list` shows them, `ipse calendar unsubscribe calendar-2` stops one (its events stay).

In `ipse ui`, **Sources → Follow a calendar by its link** does the same: paste the link, give it a name. The link goes to your system's keychain and is not shown again; the screen syncs a calendar now and stops following one.

**With the desktop app, there is nothing to set up:** while it runs, your calendars sync on their own, each at its interval (15 minutes by default), one at a time. Closing the window keeps Ipse running from its icon in the menu bar (macOS) or the tray (Windows, Linux); its menu syncs everything now, opens the window or quits. When a calendar keeps failing, a notification says so and a mark appears beside **Sources**, which says what to do. **Settings → In the background** turns the notifications off and makes Ipse start with the computer, without a window.

Without the app, have your computer run `ipse sync --due` every 5 minutes: it syncs each calendar when its interval (15 minutes by default, `--every` to change it) has passed, and says nothing when nothing changed.

- **macOS:** save this as `~/Library/LaunchAgents/org.ipse.sync.plist`, with your user name, then run `launchctl load ~/Library/LaunchAgents/org.ipse.sync.plist`. The first time, macOS asks whether `ipse` may use your keychain: choose **Always Allow**.

  ```xml
  <?xml version="1.0" encoding="UTF-8"?>
  <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
  <plist version="1.0">
  <dict>
    <key>Label</key><string>org.ipse.sync</string>
    <key>ProgramArguments</key>
    <array>
      <string>/Users/you/.ipse/bin/ipse</string>
      <string>sync</string><string>--due</string>
      <string>--vault</string><string>/Users/you/Ipse</string>
    </array>
    <key>StartInterval</key><integer>300</integer>
  </dict>
  </plist>
  ```

- **Linux:** run `crontab -e` and add this line, with your user name. Your keyring (GNOME Keyring or KWallet) must be unlocked, which it is while you are logged in.

  ```
  */5 * * * * /home/you/.ipse/bin/ipse sync --due --vault /home/you/Ipse
  ```

- **Windows:** in PowerShell:

  ```powershell
  schtasks /Create /SC MINUTE /MO 5 /TN "Ipse sync" /TR "$HOME\.cargo\bin\ipse.exe sync --due --vault $HOME\Ipse"
  ```
