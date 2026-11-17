# What Ipse protects, and what it cannot

Ipse holds a lot about your life. This page says plainly who can see what, what leaves your computer, and where the risks are.

## What stays on your computer

- **Your memory** is a folder of Markdown files on your disk. Ipse has no server and no account; nothing is copied anywhere by Ipse.
- **No telemetry.** Ipse sends no statistics, crash reports or usage data, to anyone.
- **The network** is used only for what you set up: fetching a calendar you subscribed to (over HTTPS), and, later, other sources you connect. The secret link of a calendar is kept in your system's keychain, never in your memory's files.
- **`ipse ui`** and **`ipse mcp --http`** listen on this computer only (`127.0.0.1`). The interface asks for a one-time code, then a session; the MCP server asks for each AI's own code.
- **The desktop app** shows two things in its window: its own first screen and your memory's interface, served by the `ipse` command it ships with on `127.0.0.1` and stopped when the app quits. A link to any other site opens in your browser, never in the app. The app makes no network call of its own.
- **Protection at rest** is your system's disk encryption (FileVault on macOS, BitLocker on Windows, LUKS on Linux): turn it on. Ipse keeps your files readable in any editor, so it does not encrypt them itself (decision 0009).

## What an AI sees

- **Only what you allowed**, category by category, for each AI. A category it may not read does not exist for it: no result, no count, no hint. Sensitive categories (location, home, health, private) are closed until you open them.
- **An AI that runs online sends what it reads to its maker.** If you connect Claude, ChatGPT or another online AI, what Ipse shows it leaves your computer, under that company's terms. For sensitive categories, prefer an AI that runs on your computer, such as one in LM Studio (`ipse client add "LM Studio" --local`).
- **Every read is logged** in `.ipse/access/<month>.jsonl`: which AI, which tool, which categories, how many results; never what it asked or what it got. **IA connectées** (**Connected AIs**) shows each AI's last access, and **Historique** (**History**) what each one read, day by day.

## What an AI can change

Nothing, on its own. Everything an AI wants to add or change is a proposal you accept, refuse or undo (decision 0005), whatever rights you gave it, even an AI on your computer. Every accepted change is in **Historique** (**History**) and can be undone.

## Text written by others

Some of your memory was written by other people: the title and description of an invitation someone sent you, a contact card someone shared. Text like "ignore your instructions and…" hidden there is an attack on AI agents, called **prompt injection**: it needs nothing more than sending you an invitation.

Ipse cannot tell a hostile text from an honest one, and does not try. It does three things (decision 0014):

- it gives that text to your AI as data, each piece in its own field, never as if Ipse said it;
- it tells your AI where each entry came from (you, a proposal you accepted, or an import), and who sent an invitation when it was not you;
- it tells your AI, in its instructions, never to follow instructions found in your memory, and to tell you instead.

What is left is up to your AI and to you. An AI that can act on its own elsewhere (send emails, run commands) should ask you before acting on something it read in your memory. If your AI says it found something odd in your memory, look at it in `ipse ui`.

## If you find a flaw

Do not open a public issue: use **Report a vulnerability** in the [Security tab of the repository](https://github.com/Loren844/ipse/security) on GitHub, which only the project's owner sees.
