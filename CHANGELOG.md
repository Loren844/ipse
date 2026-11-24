# Changelog

What changed in each version of Ipse, for the people who use it. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/). Before 1.0, a minor version may change the format of memory files; Ipse then says what to do.

## 0.3.0 - 2026-11-24

The desktop app lives in the background: calendars and contacts accounts sync on their own while it runs, it tells you what your AI proposes, and it keeps itself up to date once you agree. It brings the end of milestone 3: views by kind of event, the whole memory as a graph, consent that stands for summaries. The memory's format does not change: a 0.2.0 memory opens as it is. 0.2.0 does not update itself: install 0.3.0 from its file once; later versions are offered in the app.

### Added

- **Views by kind of event**: the agenda's list, week and month show one kind at a time (meals, trips, sport…), kept in the address; so does the list of events on a person's or a place's page. Each kind has its page, opened from the filter or from any kind tag: the next and the last ones, who you share them with and where, most first; how long each trip lasted; and, for celebrations, the birthdays of the next twelve months from your contacts, with the age they turn ([#134](https://github.com/Loren844/ipse/issues/134)).
- **Calendars and contacts accounts** (CalDAV and CardDAV): connect iCloud, Fastmail, Nextcloud or any such server with its address, your user name and an app password, then choose which calendars and address books to follow. Moved events and edited contacts reach your memory at the next sync, without exporting a file again; only what changed is read. An event or a contact already imported from a file is found again, not doubled. Ipse only reads, and the password stays in your system's keychain. In **Sources → Connect a calendars and contacts account**, or `ipse dav add <address> --user <name>` ([#154](https://github.com/Loren844/ipse/issues/154)).
- **Sync in the background**, in the desktop app: the calendars you follow sync on their own at their interval, one at a time, while the app runs. Closing the window keeps Ipse in the menu bar (macOS) or the tray (Windows, Linux), with open, sync now and quit; it says so the first time. A system notification tells you when your AI proposes something new to review, or when a calendar keeps failing, and a mark beside **Sources** stays until it syncs again. **Settings → In the background** turns the notifications off, and makes Ipse start with the computer, without a window (off until you turn it on). Nothing runs between two syncs ([#153](https://github.com/Loren844/ipse/issues/153)).
- **Updates**, in the desktop app: at its first start, Ipse asks whether to check for new versions (once at start and once a day, from GitHub, nothing else sent). A new version shows at the top of the window with what changes, and installs only when you click **Install and restart**: its signature is checked first, then Ipse opens again in the new version. **Later** hides it until the next check. A version that changes the format of your memory's files says so before you install it. **Settings → Check for updates** turns it on or off ([#156](https://github.com/Loren844/ipse/issues/156)).
- **Accept automatically**, per connected AI: its summaries and its event kinds, each a switch in **IA connectées**, off until you turn it on. Ipse offers it in **À revoir** once you have accepted three in a row, never before. What is accepted this way is recorded in the history like any accepted proposal, does not wait in the inbox, is counted on the home screen, and is undone in one click. Facts, people, relations and anything else from the same AI still wait for you (decision 0017, [#173](https://github.com/Loren844/ipse/issues/173)).

### Changed

- **A little motion**: a new screen settles in, a week or a month of the agenda and a year of the timeline fade in, menus, dialogs and the phone's More panel open and close softly, a notice appears, an accepted or refused proposal leaves the list and the others close the gap. Each lasts 150 to 220 ms and never holds a click back; nothing moves on its own, and nothing at all when your system asks for less motion ([#169](https://github.com/Loren844/ipse/issues/169)).
- **The graph shows your whole memory**: you at the center, people, places, organizations, works, topics and events each in a region of their own, with the links between them, faint until you choose a file. Move and zoom freely, with the mouse, a pinch or the keyboard; far out, each region shows its most linked files and how many it holds, and names appear as you zoom in. Choosing a file shows its links in a panel beside the drawing, as a list, with its page one click away. It opens in **Mémoire** with **Vue graphe**, beside the list (instead of it on a phone): a file found by the search is chosen in the graph. Each kind has its shape; what your AIs cannot see is drawn hollow. Filters by kind and category, as before. **Graph** left the side bar; **See in the graph**, on a file, opens it there ([#174](https://github.com/Loren844/ipse/issues/174)).
- A repeating event (yoga every Tuesday, a weekly meeting) shows once on the home screen, in the agenda's list and on a person's or a place's page: "chaque mardi, 19:00", at its next occurrence, and unfolds to show them all. A series is the occurrences of one calendar event, or the same title at the same weekday and time three weeks in a row. The week and month views keep every occurrence, and series still count among the people seen most ([#175](https://github.com/Loren844/ipse/issues/175)).
- The demo memory has a location history: its places sit on the map, dinners and yoga classes link to where they happened, and a person's page says where you see each other; the music of the moment, a band and a singer, has its own files ([#170](https://github.com/Loren844/ipse/issues/170)).
- Each release is checked on Windows, macOS and Linux once its installers are attached: the app installed from them opens the invented memory and quits cleanly, and Claude Desktop, connected by the installed command, reaches Ipse over MCP. What still needs a person is listed in [Milestone 3: the app, end to end](docs/acceptance/milestone-3.md) ([#157](https://github.com/Loren844/ipse/issues/157)).
- A release's notes start with the desktop app: one link per system to its installer and how to open it the first time; the command-line installers come after, as such ([#168](https://github.com/Loren844/ipse/issues/168)).
- **Mémoire de …**, at the bottom of the side bar, opens a menu: your file, the settings and, in the app, another memory; the settings left the side bar. On a phone, **Plus** shows your initials and starts with your file and the settings ([#176](https://github.com/Loren844/ipse/issues/176)).
- In the desktop app, the interface's own words (menu, buttons, headings) are no longer selected like a web page's, and links and the logo can no longer be dragged out of the window; notes, facts, summaries and fields can still be copied ([#167](https://github.com/Loren844/ipse/issues/167)).

### Fixed

- The graph, beside the memory's list, opens centred in what the window shows of it: on a window 800 pixels high, the whole memory was drawn half below the bottom of the window until the page scrolled ([#198](https://github.com/Loren844/ipse/issues/198)).
- A test of the background sync no longer fails now and then on macOS: it waits for the sync's state as it waits for its run.
- In **En ce moment**, a book or a series linked to its file lines up with the rest; the light theme's darkest red (links, names) is softened to the same family as the rest ([#171](https://github.com/Loren844/ipse/issues/171)).
- Widening the window while the phone's **Plus** panel is open no longer leaves the page blocked behind it ([#177](https://github.com/Loren844/ipse/issues/177)).

## 0.2.0 - 2026-11-17

Ipse as a desktop app: a window of its own, installed from a file, without a terminal or a browser. It brings the first part of milestone 3 (the app, its installers, the agenda as a calendar, the timeline and the map) and the end of the launch readiness stage (location history, an AI connected in one command). The memory's format does not change: a 0.1.0 memory opens as it is.

### Added

- Installers for the desktop app, attached to each release: `.dmg` for Macs with an Apple or Intel chip, `.msi` for Windows, `.deb` and `.AppImage` for Linux, each with the `ipse` command your AI apps start. They are not signed yet: [Install Ipse](docs/guide/install.md) says how to open them the first time ([#155](https://github.com/Loren844/ipse/issues/155)).
- A **Map** of the places you have been, each sized by how often you went: a place shows your visits there and the people you saw there most. It works offline with points only; OpenStreetMap's base map shows only once you accept it, and the threat model says what it reveals ([#135](https://github.com/Loren844/ipse/issues/135)).
- A **Timeline** (Frise) of your life: each year with a strip of its months showing how full they were and the people you saw most; a year opens its months with their summaries, trips and celebrations; a month opens what marked it, then its calendar ([#133](https://github.com/Loren844/ipse/issues/133)).
- The agenda as a calendar: **Week** and **Month** views beside the list, with today marked, earlier and later periods, each event's time and kind, and a day's summary from its day. On a phone the week is a list of days and the month counts each day's events ([#132](https://github.com/Loren844/ipse/issues/132)).
- A **Sources** screen: import a contacts file, a calendar or your location history by dropping it on the page, follow a calendar by its secret link (kept in your system's keychain), sync it now or stop following it, and see what each source brought and when ([#152](https://github.com/Loren844/ipse/issues/152)).
- The desktop app: Ipse in its own window, with no terminal and no browser. Its first screen creates your memory, makes the invented one to try Ipse, or opens a folder; it then opens the same memory at each start, and **Settings** goes back to choose another. Installers come next ([#151](https://github.com/Loren844/ipse/issues/151)).
- Import your phone's location history (Google Maps Timeline, from Android, iPhone or Takeout): `ipse import Timeline.json`. The places you go become places of your memory, the events of your agenda get their place, other outings become events, and a person's page shows where you meet. All of it is in the `location` category, closed to your AIs until you open it ([#106](https://github.com/Loren844/ipse/issues/106)).
- Connect an AI app in one command: `ipse client add <name> --install claude-desktop`, `claude-code` or `lm-studio` writes Ipse into that app's configuration, its access code included, without showing it; the app's other servers stay, and the file as it was is kept. In `ipse ui`, **Connect an AI** offers the same for the apps found on the computer. LM Studio now starts Ipse itself: no terminal to leave open.

### Fixed

- The Mémoire screen's search box can be typed in again, also after a search from the side bar ([#147](https://github.com/Loren844/ipse/issues/147)).

## 0.1.0 - 2026-11-17

The first version of Ipse: a memory you can see whole, change yourself and explore through its links, not only query through an AI ([decision 0016](docs/decisions/0016-a-first-version-shows-the-whole-memory.md)). It holds milestones 1 and 2, the first part of the launch readiness stage ([decision 0015](docs/decisions/0015-launch-with-the-desktop-app.md)) and the First version milestone.

### Your memory

- A memory is a folder of Markdown files you own, readable in any editor or in Obsidian: people, places, events, summaries ([vault format](docs/spec/vault.md)). `ipse init` creates one; `ipse init --demo` an invented one, to try Ipse first.
- Every change is recorded in an invisible history, with who made it (you, a source, an AI's proposal you accepted), and can be undone.
- Search by name or by any word of what is known, in a few milliseconds on tens of thousands of files.

### Sources

- Contacts from vCard files (Google, Apple, Outlook, vCard 2.1 to 4): names, emails, phones in international form, birthdays, addresses, organizations, notes, and the related names on your own card ("mother: Mom") as relations.
- Calendars from iCalendar files, or kept in sync by subscribing to their secret link (kept in your system's keychain): guests linked to the people you know, repeating events, time zones, cancellations.
- A change you made yourself is never overwritten by a source: what the source says waits for your decision.
- Two files of the same person or event are never merged without you: possible duplicates are proposed.

### Your AI

- Any AI app that speaks MCP connects with its own code (`ipse client add`), locally or over HTTP on this computer, and reads only the categories you allowed; sensitive ones are closed by default. Every read is logged, without what was asked.
- With your permission, it tidies up your memory through tasks: facts and relations from notes, merges of duplicates, the full names of people saved under a nickname, summaries of your days, weeks, months and years. Everything it wants to change is a proposal you accept, refuse (saying why) or undo.
- Text imported from your apps reaches your AI as data, never as instructions, with where it came from and who sent an invitation ([decision 0014](docs/decisions/0014-imported-data-is-untrusted.md)).

### The interface

- `ipse ui` opens your memory in your browser, on this computer only: home, the whole memory, agenda, summaries, review inbox with what each proposal changes, connected AIs and their rights per category, history of changes, of AI reads and of imports. French and English.
- The whole memory, not only people: places, organizations, projects, works, things and topics, each listed and searchable, with what links to each file; a search across everything from every screen; what is going on right now (music, what you watch or read).
- Settings: a light or dark theme (or the system's), the interface's language, the memory's language and time zone.
- Edit any file yourself in `ipse ui`: names, phones, emails, birthday, address, an event's times, place and people, facts, relations and notes. Create a file of any kind, including the one a link already points to, and delete one after seeing what links to it. Each save is one change you can undo from the history; a file changed meanwhile is never written over.
- A graph around any file, yourself by default: what it links to and what links to it, one or two steps away, filtered by kind of file and category, the categories closed to AIs marked. Click a file to put it at the center. The same neighbourhood reads as a list, for the keyboard and screen readers.
- Kinds of events: meals, celebrations, outings, culture, sport, trips, travel, work, studies, appointments, health. Your AI proposes them, a few batches a day, and you accept them in the review inbox; you can also set one on the event's page. The agenda shows them.
- On a phone, the main sections (home, memory, agenda, review) sit in a bar at the bottom, the others behind More, and the search stays in the top bar on every screen.

### Installing

- Binaries for Windows, macOS (Apple silicon and Intel) and Linux (x64 and ARM), with an installer for each system ([Getting started](docs/guide/getting-started.md)).

## 0.1.0-rc.1 - 2026-11-16

A pre-release of 0.1.0, to try the binaries and the installers before the first version. What it holds is described under 0.1.0 above.
