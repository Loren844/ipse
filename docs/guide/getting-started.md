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

## 4. Connect your AI

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

Quit Claude Desktop completely and open it again. Ipse appears among the connectors, with three tools: `about_me`, `search` and `get`.

## 5. Ask

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
- **Logs:** Claude Desktop keeps what Ipse says in `mcp-server-ipse.log`, in `~/Library/Logs/Claude` on macOS and `%APPDATA%\Claude\logs` on Windows.
