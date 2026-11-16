# Review what it proposes

Nothing an AI suggests is written to your memory until you say yes. Proposals wait for you in one place; each one can be accepted, refused or, once accepted, undone.

## Where

Open `ipse ui --vault ~/Ipse`. **À revoir** (**To review**) shows how many proposals wait, in the menu and on the home screen. From the command line: `ipse proposals list`, then `show`, `accept`, `refuse` or `undo` followed by the proposal's first characters.

## What a proposal says

- **Who** proposed it: the AI by the name you gave it, Ipse itself (possible duplicates), or one of your sources that changed a file you had edited.
- **Why**, in the AI's words: "Marc told me during our conversation."
- **What it changes**, file by file: what would be added in green with a +, what would go in red with a −.

## Deciding

- **Accepter** (**Accept**): the files change, as one change in **Historique** (**History**).
- **Several at once**: tick them in the list, then **Accepter les propositions sélectionnées**. Each is still its own change, and can be undone on its own.
- **Refuser** (**Refuse**), with a reason if you like: "She is a midwife, not a nurse." Nothing changes; your AI reads your answer and the task comes back to it.
- **Annuler** (**Undo**), on a proposal you accepted (under **Décidées récemment**, or in **Historique**): the files are back as they were before it. Undoing is a change too; history keeps both.
- **Ne s'applique plus** (**No longer applies**): the files changed since the AI proposed it. It can only be refused; ask your AI again.

## Two kinds that read differently

- **The same person twice.** The proposal shows both files and what each brings. **Fusionner** (**Merge**) keeps one file with everything from both, and the links to the other now point to it. **Ce sont deux personnes** (**These are two people**) keeps them apart for good: Ipse will not propose them again.
- **Qui est-ce ?** (**Who is this?**). People saved under a first name or a nickname, with what Ipse knows about them. Type their full name and, if you like, who they are to you ("plumber", "sister"). **Je ne sais pas** (**I don't know**) and you are not asked again.

## Changing your mind later

Every change, yours, your AI's or your sources', is listed in **Historique** (**History**), with who made it, and can be undone from there. The files are plain Markdown: you can always read them, or edit them in any editor.
