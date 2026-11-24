# Connecting AI apps

[Getting started](getting-started.md) connects Claude Desktop and LM Studio in one command. This page covers the other cases, and how to change what each AI sees.

## Apps Ipse sets up itself

`ipse client add <name> --install <app>` writes Ipse into the app's MCP configuration, with the access code, which is then shown nowhere:

| `<app>` | File |
| --- | --- |
| `claude-desktop` | macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`; Windows: `%APPDATA%\Claude\claude_desktop_config.json`; Linux: `~/.config/Claude/claude_desktop_config.json` |
| `claude-code` | `~/.claude.json` (every project) |
| `lm-studio` | `~/.lmstudio/mcp.json` (Windows: `%USERPROFILE%\.lmstudio\mcp.json`) |

Only Ipse's entry, `ipse`, is added or replaced: the app's other servers and settings stay. The file as it was is kept next to it, with `.before-ipse` added to its name. A file Ipse cannot read as JSON (one with comments, say) is left alone, and the AI is not connected. In `ipse ui`, **Connect an AI** offers the same for the apps found on this computer.

## An app set up by hand

```sh
ipse client add "My app" --vault ~/Ipse
```

shows the code once. Most apps take a JSON entry like this one in their `mcpServers`, with **full paths** (`~` does not work there; on Windows, `"C:\\Users\\you\\Ipse"`):

```json
{
  "mcpServers": {
    "ipse": {
      "command": "/Users/you/.ipse/bin/ipse",
      "args": ["mcp", "--vault", "/Users/you/Ipse"],
      "env": { "IPSE_TOKEN": "ipse_paste_your_code_here" }
    }
  }
}
```

## An AI app that connects over HTTP

Some AI apps connect to MCP servers by address rather than by starting a program. Start Ipse for them, and leave the terminal open:

```sh
ipse mcp --http --vault ~/Ipse
```

It serves `http://127.0.0.1:7420/mcp`, on this computer only (`--http 8123` for another port). In the AI app, add an MCP server of type "Streamable HTTP" (or just "HTTP") with that address and a header `Authorization` whose value is `Bearer ` followed by your code. For example, with Claude Code:

```sh
claude mcp add --transport http ipse http://127.0.0.1:7420/mcp --header "Authorization: Bearer ipse_paste_your_code_here"
```

A request without the code, or with a wrong one, is refused before anything is read.

To reach your memory from other machines, run it on a home server instead: [Ipse on a home server](self-hosting.md) serves MCP at `/mcp` beside the interface, with the same codes.

## Changing what an AI sees

The simplest way: `ipse ui --vault ~/Ipse`, then **IA connectées** (**Connected AIs**). Each AI has a switch per category, to read and to propose; a sensitive category asks before opening. You can also connect an AI there (its code is shown once, with what to paste in Claude Desktop or LM Studio), disconnect one, and add your own categories. From the command line:

- `ipse client list --vault ~/Ipse` shows the connected AIs and what each may read.
- `ipse client add "Ollama" --local` connects an AI that runs entirely on your computer, such as a local model: it is shown as such. It still reads only the non-sensitive categories until you open others to it.
- Every call an AI makes is written to `.ipse/access/<month>.jsonl`: which categories it read and how many results, never what it asked or what it got. Ipse keeps 12 months.
- Your own categories go in `.ipse/vault.toml`, for example `[categories.studies]` with `sensitive = false`. AIs connected afterwards read it; for those already connected, add it to their `read` list.
- `ipse client set claude --read identity,people,agenda --vault ~/Ipse` changes what an AI may read, from its next request.
- `ipse client set claude --propose people --vault ~/Ipse` lets it **propose** changes in your contacts: facts it read in their notes, things you told it. Nothing is written until you accept them: open `ipse ui` and go to **À revoir** (**To review**), or use `ipse proposals list`, then `accept` or `refuse`. No AI can propose by default.
- `ipse client set claude --propose people,summaries --vault ~/Ipse` also lets it write the **summaries** of your days, weeks, months and years: ask it to "summarize my last days", then accept them in `ipse ui`, where **Résumés** shows them by day, week, month and year. They are searchable, and you can rewrite them: Ipse never writes over your words without asking.
- `ipse client remove claude --vault ~/Ipse` disconnects one: its code stops working at its next request.
- An AI that may not read a category gets the same answers as if that data did not exist.
