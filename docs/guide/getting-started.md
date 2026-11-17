# Getting started

Import your contacts and your calendar, connect your AI, and ask it about your week. Everything stays on your computer: your AI app reads your memory through Ipse, which only shows it what you allow.

You need a terminal, and [Claude Desktop](https://claude.ai/download) or [LM Studio](https://lmstudio.ai) (0.3.17 or later).

## 1. Install Ipse

**The desktop app** is the simplest: download it for your system and open it, as [Install Ipse](install.md) shows. It does everything below without a terminal. This guide uses the `ipse` command, for those who prefer it.

On macOS or Linux, in a terminal:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Loren844/ipse/releases/latest/download/ipse-installer.sh | sh
```

On Windows, in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/Loren844/ipse/releases/latest/download/ipse-installer.ps1 | iex"
```

The installer puts the `ipse` command in `~/.ipse/bin` (on Windows, `%USERPROFILE%\.ipse\bin\ipse.exe`) and adds it to your path: open a new terminal and check with `ipse --version`. You can also download the file for your system from the [releases page](https://github.com/Loren844/ipse/releases), and check it against its `.sha256`. To build it yourself, see [CONTRIBUTING.md](../../CONTRIBUTING.md).

## Try it first, without your data

```sh
ipse init ~/Ipse-demo --demo
ipse ui --vault ~/Ipse-demo
```

This makes an invented memory, Camille Laurent's (Alex Morgan's with `--language en`): about 150 people, two years of agenda, summaries of the last days, and proposals waiting for a decision as if an AI had made them. Nothing in it is real: emails are at `example.org`, phone numbers are in ranges kept for fiction. Delete the folder when you are done.

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

The interface does the same without a terminal: in `ipse ui`, **Sources** takes a file dropped on it, says how to export each one from the usual apps, and lists what feeds your memory.

Your phone's location history can come too: export it from Google Maps Timeline on the phone ([how](../connectors/location.md#getting-the-file)), then `ipse import Timeline.json --vault ~/Ipse`. The places you go become files in `places/`, and the events of your agenda get their place. All of it stays in the `location` category, which your AIs cannot see until you open it.

## 4. Connect Claude Desktop

```sh
ipse client add "Claude" --install claude-desktop --vault ~/Ipse
```

Ipse connects an AI named Claude, with what it may read (sensitive categories, such as location, home, health and private, stay closed), and writes itself into Claude Desktop's configuration, its access code included. The code is shown nowhere. The file as it was is kept next to it (`claude_desktop_config.json.before-ipse`).

Quit Claude Desktop completely and open it again: Ipse appears among its connectors.

`--install claude-code` does the same for Claude Code, in every project. To connect an app by hand instead, see [Connecting AI apps](connecting.md). You can also connect an AI from `ipse ui` (step 7).

## 5. Or connect a local AI: LM Studio

A model that runs in LM Studio (0.3.17 or later) stays on your computer:

```sh
ipse client add "LM Studio" --install lm-studio --vault ~/Ipse
```

LM Studio reloads its MCP servers: Ipse appears in the **Program** tab of the right sidebar. Pick a model that supports tools. An AI connected this way is marked as running on this computer.

## 6. Ask

"Who am I?", "Who is Marie?", "What am I doing this weekend?" Your AI searches your memory and answers. It may ask you to allow each tool the first time.

## 7. See it yourself

```sh
ipse ui --vault ~/Ipse
```

Your browser opens on your memory: the people you know, what is coming, what happened, and what your AI proposes to add, waiting for your decision. It runs on this computer only; close it with Ctrl+C in the terminal.

## Going further

- [Keep your calendar in sync](calendar.md): subscribe to your calendar's link.
- [Let your AI tidy up your memory](tidy-up.md): notes, nicknames and summaries, done by your AI.
- [Review what it proposes](review.md): accept, refuse, undo.
- [Connecting AI apps](connecting.md): other apps, and changing what each AI sees.
- [What Ipse protects, and what it cannot](threat-model.md).
- [If it does not work](troubleshooting.md).
