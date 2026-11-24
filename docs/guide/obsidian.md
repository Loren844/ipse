# Open your memory in Obsidian

Your memory is a folder of Markdown files, each with a YAML header, linked with `[[links]]`. Obsidian opens it as it is: you can read it, follow the links and see its graph, and edit it alongside Ipse. Nothing needs converting, and Ipse needs no plugin.

## Open the folder as a vault

1. Find your memory's folder: in Ipse, **Réglages** (**Settings**) shows it under **Dossier de la mémoire** (**Memory folder**), with a button to copy it. It is also on the **Comment ça marche** (**How it works**) page.
2. In Obsidian: **Open folder as vault**, then pick that folder.
3. In Obsidian's **Settings → Files and links → Excluded files**, add `.ipse/` and `journal/`. They hold Ipse's own data and the raw imports, not your memory. Obsidian then leaves them out of search and the graph.

Each file opens with its header shown as **Properties**: `name`, `aliases`, `emails`, `phones`, `birthday`… The folders are the kinds of file: `people/`, `places/`, `organizations/`, `projects/`, `things/`, `topics/`, `works/`, `reminders/`, `events/<year>/` and `summaries/`.

## What you can change by hand

Ipse watches the folder. When you save a file in Obsidian, Ipse records it as your change in **Historique** (**History**), where you can undo it like any other. **You always win:** Ipse never writes over an edit it did not make, and an import that disagrees with you becomes a proposal in **À revoir** (**To review**).

Safe to edit:

- **Facts**: one line each under `## Faits` (or `## Facts`), such as `- Allergic to nuts.`. Ipse adds a short code at the end of a line it reads, like `^f-3k9w`. It is an Obsidian block id, hidden in reading view: leave it in place.
- **Relations**: one line each under `## Relations`, a word then a link, such as `- sister: [[ellie-morgan]]`.
- **Notes**: anything under `## Notes`.
- **Properties**: `name`, `aliases`, `emails`, `phones`, `address`, `birthday` (`1965-03-12`, or `--03-12` without the year); for an event, `title`, `start`, `end`, `place`, `participants`. A reminder takes `at`, `every` and `done`.
- **Your own notes**: files outside these folders, such as `Notes.md` at the top of the vault or a folder of yours, are left alone. Ipse neither reads nor indexes them. To bring a fact from one into your memory, ask your AI: it proposes it, and you accept it.

Leave alone:

- `id`, `type`, `category` and `owner` in the header. `id` is how Ipse knows a file even when it moves. To change a file's category, use **Modifier** (**Edit**) in Ipse.
- `created`, `updated` and `sources`, which Ipse keeps.
- The `.ipse/` and `journal/` folders.

If a file no longer reads after an edit (a header that is not valid YAML, a date in the wrong form), Ipse skips it and keeps the rest. `ipse check --vault <folder>` names each such file, the line, and what to fix. In the app, **Modifier → Markdown** checks your text before saving.

## Links and names

- A link is the file's name without `.md`: `[[marie-dupont]]`, or `[[marie-dupont|Maman]]` to show another text. Obsidian follows these links, and its graph view shows them.
- **Rename a person or a thing from Ipse** (**Modifier**, then the name): Ipse renames the file and rewrites every link to it, in one change you can undo. If you rename a file in Obsidian instead, turn on **Settings → Files and links → Automatically update internal links**, so that Obsidian updates the links as well. Ipse still knows the file by its `id`.
- **Don't duplicate a file** in Obsidian (**Make a copy**): the copy keeps the same `id`, and Ipse would see one file twice. Create a new one from Ipse (**Ajouter**) or ask your AI.

## Syncing between computers

The memory is plain files, so Obsidian Sync, Syncthing or iCloud Drive can copy it to your other devices, to read and edit there. Ipse itself is made to run on one computer at a time: open the app on one of them only, and let it pick up the edits that arrive. Exclude `.ipse/index/` from your sync tool if it lets you: Ipse rebuilds it.

The memory's history, which lets you undo any change, is a hidden `.git` folder at the top of the vault. Obsidian's own settings, in `.obsidian/`, become part of that history too, except its layout files (`.obsidian/workspace*`) and its `.trash/` folder, which Ipse leaves out.
