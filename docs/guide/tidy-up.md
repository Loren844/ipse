# Let your AI tidy up your memory

After an import, your memory holds what your apps knew, as they knew it: notes nobody has read, people saved as "Jo plombier", days with events but no account of them. Your AI can do that work. Ipse leaves it a list of tasks; the AI reads them and **proposes** changes. Nothing is written until you accept: see [Review what it proposes](review.md).

## 1. Let it propose

By default, an AI may read but not propose. Open `ipse ui --vault ~/Ipse`, go to **IA connectées** (**Connected AIs**), and turn on **Proposer** (**Propose**) for the categories it may work on: usually Personnes, Agenda and Résumés. Or, from the command line:

```sh
ipse client set claude --propose people,agenda,summaries --vault ~/Ipse
```

It still sees only the categories it may read, and it gets no task about the others.

## 2. Ask it

In your AI app, ask something like "Tidy up my memory" or "Summarize my last days". It goes through what Ipse left for it:

- **Notes to read.** A contact's notes or an event's description become facts and relations: "Nurse in Nantes", "sister of Marc".
- **Who is this?** For someone saved under a first name, a nickname or "Mum", it asks you, in one short question, using what Ipse knows (phone, email, the last events with them). Your answer becomes their full name; the old one stays as another name, so searching for it still finds them.
- **Summaries.** The days that ended, then the weeks, months and years once their days are written, in your memory's language.
- **The same person twice.** Ipse finds people sharing an email or a phone on its own, after each import. Your AI can propose others it recognizes, saying why.

Questions come a few at a time, at most three a day, and nobody is asked about twice.

## 3. Accept what is right

Each proposal waits in **À revoir** (**To review**), with why your AI proposed it and exactly what it would change. When you refuse one and say why, your AI reads your answer and the task comes back to it, so it can do better.

You can ask again whenever you like: there is only work when something new arrived.

## Good to know

- An AI that runs online sends what it reads to its maker. For sensitive categories, prefer an AI that runs on your computer, such as one in LM Studio (`ipse client add "LM Studio" --local`).
- `ipse tasks list --vault ~/Ipse` shows the tasks as your AI sees them.
