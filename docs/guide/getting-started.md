# Getting started

Ask your AI who someone is, and let it answer from your own contacts. This page takes about fifteen minutes. Everything stays on your computer: your AI app reads your memory through Ipse, which only shows it what you allow.

You need [Claude Desktop](https://claude.ai/download) or another AI app that supports MCP, and a terminal.

## 1. Install Ipse

Ipse has no installer yet. Build it from source:

1. Install Rust from [rustup.rs](https://rustup.rs). The right version is installed automatically on the first build.
2. Get the code and install the `ipse` command:

   ```sh
   git clone https://github.com/Loren844/ipse
   cd ipse
   cargo install --path crates/ipse
   ```

The command is now in `~/.cargo/bin/ipse` (on Windows, `%USERPROFILE%\.cargo\bin\ipse.exe`). Check with `ipse --version`.

## 2. Create your memory

```sh
ipse init ~/Ipse
```

(On Windows, in PowerShell, write `$HOME\Ipse` instead of `~/Ipse`, here and below.)

Ipse asks your name and creates a folder of plain Markdown files, readable in any editor or in Obsidian. Your own file is `people/<your-name>.md`: write there what your AI should know about you.

## 3. Import your contacts

Export your contacts as a vCard file (`.vcf`):

- **Google Contacts:** on [contacts.google.com](https://contacts.google.com), select the contacts, then **Export → vCard**.
- **Apple Contacts:** on a Mac, select all contacts, then **File → Export → Export vCard**. From an iPhone, do the same on [icloud.com/contacts](https://www.icloud.com/contacts).

Then:

```sh
ipse import contacts.vcf --vault ~/Ipse
```

Each contact becomes a file in `people/`. Importing the same file again later updates them; people who look alike are never merged without asking you.

Before importing, add your email to your own file (a line `emails:` then `  - you@example.org` in its header). Ipse then recognizes your own card: it completes your file instead of making a second one, and the related people it lists ("mother: Maman") become your relations.

## 4. Subscribe to your calendar

Your AI can also know what you did and what is coming. Copy your calendar's link in iCal format:

- **Google Calendar:** on [calendar.google.com](https://calendar.google.com), **Settings**, pick your calendar under "Settings for my calendars", then **Integrate calendar → Secret address in iCal format**.
- **Outlook:** **Settings → Calendar → Shared calendars → Publish a calendar**, choose "Can view all details", then copy the ICS link.
- **iCloud:** in the Calendar app, the calendar's menu → **Share Calendar → Public Calendar**. Apple makes the calendar readable by anyone who has this link.

Whoever has the link can read the calendar: Ipse keeps it in your system's keychain, never in your memory's files. Then:

```sh
ipse calendar subscribe "<the link>" --name Perso --vault ~/Ipse
```

Each event becomes a file in `events/`, its guests linked to your contacts. `ipse sync --vault ~/Ipse` syncs your calendars again; `ipse calendar list` shows them, `ipse calendar unsubscribe calendar-2` stops one (its events stay).

To keep them in sync on their own until the app does it, have your computer run `ipse sync --due` every 5 minutes: it syncs each calendar when its interval (15 minutes by default, `--every` to change it) has passed, and says nothing when nothing changed.

- **macOS:** save this as `~/Library/LaunchAgents/org.ipse.sync.plist`, with your user name, then run `launchctl load ~/Library/LaunchAgents/org.ipse.sync.plist`. The first time, macOS asks whether `ipse` may use your keychain: choose **Always Allow**.

  ```xml
  <?xml version="1.0" encoding="UTF-8"?>
  <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
  <plist version="1.0">
  <dict>
    <key>Label</key><string>org.ipse.sync</string>
    <key>ProgramArguments</key>
    <array>
      <string>/Users/you/.cargo/bin/ipse</string>
      <string>sync</string><string>--due</string>
      <string>--vault</string><string>/Users/you/Ipse</string>
    </array>
    <key>StartInterval</key><integer>300</integer>
  </dict>
  </plist>
  ```

- **Linux:** run `crontab -e` and add this line, with your user name. Your keyring (GNOME Keyring or KWallet) must be unlocked, which it is while you are logged in.

  ```
  */5 * * * * /home/you/.cargo/bin/ipse sync --due --vault /home/you/Ipse
  ```

- **Windows:** in PowerShell:

  ```powershell
  schtasks /Create /SC MINUTE /MO 5 /TN "Ipse sync" /TR "$HOME\.cargo\bin\ipse.exe sync --due --vault $HOME\Ipse"
  ```

## 5. Connect your AI

```sh
ipse client add "Claude" --vault ~/Ipse
```

Ipse shows which categories this AI may read and an access code starting with `ipse_`. **The code is shown only once**: copy it now. Sensitive categories (location, home, health, private) stay closed.

In Claude Desktop, open **Settings → Developer → Edit Config**. This opens `claude_desktop_config.json`. Add Ipse to it, with **full paths** (`~` does not work here) and your code:

```json
{
  "mcpServers": {
    "ipse": {
      "command": "/Users/you/.cargo/bin/ipse",
      "args": ["mcp", "--vault", "/Users/you/Ipse"],
      "env": { "IPSE_TOKEN": "ipse_paste_your_code_here" }
    }
  }
}
```

On Windows, write paths with double backslashes: `"C:\\Users\\you\\.cargo\\bin\\ipse.exe"` and `"C:\\Users\\you\\Ipse"`. If the file already lists other servers, add `"ipse": {…}` next to them.

Quit Claude Desktop completely and open it again. Ipse appears among the connectors, with six tools: `about_me`, `search`, `get`, `events`, `now` and `states_history`.

### An AI app that connects over HTTP

Some AI apps connect to MCP servers by address rather than by starting a program. Start Ipse for them, and leave the terminal open:

```sh
ipse mcp --http --vault ~/Ipse
```

It serves `http://127.0.0.1:7420/mcp`, on this computer only (`--http 8123` for another port). In the AI app, add an MCP server of type "Streamable HTTP" (or just "HTTP") with that address and a header `Authorization` whose value is `Bearer ` followed by your code. For example, with Claude Code:

```sh
claude mcp add --transport http ipse http://127.0.0.1:7420/mcp --header "Authorization: Bearer ipse_paste_your_code_here"
```

A request without the code, or with a wrong one, is refused before anything is read.

## 6. Ask

- "Who am I?"
- "Who is Marie?"
- "When is Marc's birthday?"

Claude searches your memory, reads the right file, and answers. Claude Desktop may ask you to allow each tool the first time.

## Changing what an AI sees

- `ipse client list --vault ~/Ipse` shows the connected AIs and what each may read.
- `ipse client remove claude --vault ~/Ipse` disconnects one: its code stops working at its next request.
- Until the app arrives, rights are changed in `.ipse/clients.toml` inside your memory: remove a category from the `read` list of that AI, or add one. It applies at the AI's next request. An AI that may not read a category gets the same answers as if that data did not exist.

## If it does not work

- **Ipse is not among the connectors:** check that the JSON is valid and that both paths are full and correct. Try the command in a terminal: `ipse mcp --vault /Users/you/Ipse`. If the folder is wrong, it says so and stops; otherwise it waits for an AI app (stop it with Ctrl+C).
- **Claude says the access code is not valid:** the code was mistyped or the AI was removed. Run `ipse client add` again and paste the new code.
- **Ipse could not use the keychain:** on Linux, install and unlock a keyring (GNOME Keyring or KWallet); on macOS, allow `ipse` when the system asks.
- **A calendar link no longer works:** it was reset in your calendar app. Copy the new one, then `ipse calendar unsubscribe <id>` and `ipse calendar subscribe` again.
- **Logs:** Claude Desktop keeps what Ipse says in `mcp-server-ipse.log`, in `~/Library/Logs/Claude` on macOS and `%APPDATA%\Claude\logs` on Windows.
