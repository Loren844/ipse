# Connecting AI apps

[Getting started](getting-started.md) connects Claude Desktop and LM Studio. This page covers the other cases, and how to change what each AI sees.

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

## Changing what an AI sees

- `ipse client list --vault ~/Ipse` shows the connected AIs and what each may read.
- `ipse client add "Ollama" --local` connects an AI that runs entirely on your computer, such as a local model: it is shown as such. It still reads only the non-sensitive categories until you open others to it.
- Every call an AI makes is written to `.ipse/access/<month>.jsonl`: which categories it read and how many results, never what it asked or what it got. Ipse keeps 12 months.
- Your own categories go in `.ipse/vault.toml`, for example `[categories.studies]` with `sensitive = false`. AIs connected afterwards read it; for those already connected, add it to their `read` list.
- `ipse client remove claude --vault ~/Ipse` disconnects one: its code stops working at its next request.
- Until the app arrives, rights are changed in `.ipse/clients.toml` inside your memory: remove a category from the `read` list of that AI, or add one. It applies at the AI's next request. An AI that may not read a category gets the same answers as if that data did not exist.
