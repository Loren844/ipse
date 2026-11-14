# Getting started

Import your contacts and your calendar, connect your AI, and ask it about your week. Everything stays on your computer: your AI app reads your memory through Ipse, which only shows it what you allow.

You need a terminal, and [Claude Desktop](https://claude.ai/download) or [LM Studio](https://lmstudio.ai) (0.3.17 or later).

## 1. Install Ipse

Ipse has no installer yet. Install Rust from [rustup.rs](https://rustup.rs), then:

```sh
git clone https://github.com/Loren844/ipse
cd ipse
cargo install --path crates/ipse
```

The command is now in `~/.cargo/bin/ipse` (on Windows, `%USERPROFILE%\.cargo\bin\ipse.exe`).

## 2. Create your memory

```sh
ipse init ~/Ipse
```

(In PowerShell, write `$HOME\Ipse` instead of `~/Ipse`, here and below.) Ipse asks your name and creates a folder of Markdown files, readable in any editor. Add your email to your own file, `people/<your-name>.md` (a line `emails:` then `  - you@example.org` in its header): Ipse then recognizes your own contact card and completes your file with it.

## 3. Import your contacts and your calendar

Export them from your apps:

- **Contacts (`.vcf`):** in [Google Contacts](https://contacts.google.com), **Export → vCard**. On a Mac, in Contacts, **File → Export → Export vCard**.
- **Calendar (`.ics`):** in [Google Calendar](https://calendar.google.com), **Settings → Import & export → Export** (unzip the file it gives). On a Mac, in Calendar, **File → Export → Export**.

Then import each file. Ipse picks the right connector from the file type:

```sh
ipse import contacts.vcf --vault ~/Ipse
ipse import calendar.ics --vault ~/Ipse
```

Each contact becomes a file in `people/`, each event a file in `events/`. Importing again later updates them. To follow your calendar as it changes, see [Keep your calendar in sync](calendar.md).

## 4. Connect Claude Desktop

```sh
ipse client add "Claude" --vault ~/Ipse
```

Ipse shows what this AI may read and an access code starting with `ipse_`, **shown only once**: copy it. Sensitive categories (location, home, health, private) stay closed.

In Claude Desktop, **Settings → Developer → Edit Config** opens `claude_desktop_config.json`. Add Ipse with **full paths** (`~` does not work here; on Windows, `"C:\\Users\\you\\Ipse"`):

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

Quit Claude Desktop completely and open it again: Ipse appears among its connectors.

## 5. Or connect a local AI: LM Studio

A model that runs in LM Studio stays on your computer. Connect it with its own code, and start Ipse for it in a terminal you leave open:

```sh
ipse client add "LM Studio" --local --vault ~/Ipse
ipse mcp --http --vault ~/Ipse
```

In LM Studio, in the **Program** tab of the right sidebar, **Install → Edit mcp.json**, and add:

```json
{
  "mcpServers": {
    "ipse": {
      "url": "http://127.0.0.1:7420/mcp",
      "headers": { "Authorization": "Bearer ipse_paste_your_code_here" }
    }
  }
}
```

Pick a model that supports tools.

## 6. Ask

"Who am I?", "Who is Marie?", "What am I doing this weekend?" Your AI searches your memory and answers. It may ask you to allow each tool the first time.

## Going further

- [Keep your calendar in sync](calendar.md): subscribe to your calendar's link.
- [Connecting AI apps](connecting.md): other apps, and changing what each AI sees.
- [If it does not work](troubleshooting.md).
