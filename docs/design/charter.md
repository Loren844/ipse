# Ipse's visual identity

How Ipse looks and speaks, everywhere it appears: the app, its first screen, the website, the README. The tokens live in `ui/src/app.css` (and are copied in `app/welcome/welcome.css`); this file says what they mean and when to use them. A screen that needs something this charter does not have adds it here first.

## 1. The idea: a thread

Ipse is named *Fil*, a thread. A life is read as one line, and what happens on it is a knot. Everything else follows from that:

- **The thread** is a vertical line of madder red (`--thread-faint` for the line, `--thread` for knots). The agenda, the home screen, the history and the empty screens hang their content on it.
- **A knot** is a ring (a circle with a thick border, open in the middle). A filled knot is *now*: today, the selected file.
- **A note** is written *on the thread*: text after a 2 px red line on its left, never in a tinted box. Day summaries, offers from an AI, hints that matter.

The red is rare on purpose. If a screen has more than a few red marks, one of them is wrong.

## 2. The mark

- An **i whose stem loops back on itself**: *ipse*, oneself. Drawn on a 40 × 40 grid: a stem from the baseline up to a half-loop that curls to the right, a dot above. Stroke ends are round.
- **Wordmark**: `ipse`, lowercase, Bricolage Grotesque 800, tracking −0.03 em, always beside the mark, never alone in a sentence.
- In running text the name is **Ipse**, capitalized, never IPSE.
- The app icon is the mark in white on a `--thread` rounded square (corner 11/40). Mark and icon are the same drawing at two scales (`ui/src/lib/Mark.svelte`, `app/welcome/mark.svg`).
- The mark is `--thread` on light grounds, `--thread` (lightened in dark mode) on dark grounds. It is never outlined, shadowed, tilted, put in a gradient or used as a pattern.

## 3. Type

Two families, both shipped with the app (nothing fetched from the network):

| Role | Family | Use |
| --- | --- | --- |
| Display | **Bricolage Grotesque** (variable) | Page titles (800, tracking −0.035 em), section titles (700), big numbers (800). |
| Text | **Atkinson Hyperlegible Next** (variable) | Everything else: body, labels, buttons, fields. |

Atkinson's zero is slashed: that is part of its legibility (0 is never read as O) and is kept, in times and phone numbers too.

The scale has six steps, nothing in between:

| Token | Size | Use |
| --- | --- | --- |
| `--step--2` | 12 px | Counts in badges, month labels, map credits. |
| `--step--1` | 14 px | Meta lines (who proposed, when), hints under fields, chips. |
| `--step-0` | 16 px | Body text, navigation, buttons, fields. |
| `--step-1` | 18 px | A page's introduction (`.intro`), small headings (h3). |
| `--step-2` | 21 px | Section titles (h2), the lead sentence of a status. |
| `--step-4` | 52 px | Page titles (h1), shrinking with the window down to 36 px. |

Weights: 400 for text, 700 for emphasis and labels, 800 for display only. No italics for emphasis; no all caps.

## 4. Colour

Warm paper, ink, and one red. Dark mode is the same palette at night, not an inversion.

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--ground` | `#fbfaf8` | `#171314` | The page. |
| `--surface` | `#ffffff` | `#211b1c` | Fields, cards that hold a list. |
| `--text` | `#231d1e` | `#f0eaea` | Text and secondary buttons' labels. |
| `--muted` | `#6e6466` | `#ada2a3` | Meta lines, introductions, hints. |
| `--line` / `--line-strong` | `#eee9e7` / `#e5dedc` | `#2e2627` / `#3a3132` | Separators, borders of fields and secondary buttons. |
| `--thread` | `#9b2c3a` | `#cf6874` | The thread, knots, the primary action, focus rings, the mark. |
| `--thread-ink` | `#94313f` | `#f3c5ca` | Links and red text (contrast checked on `--ground`). |
| `--thread-soft` | `#f6e9ea` | `#3a2226` | **Selected state only**: the current section, a pressed segment, a person chip. |
| `--thread-faint` | `#e8c9cd` | `#5a3238` | The thread's line, past knots, bars of the timeline. |
| `--added` / `--added-soft` | green | green | What a proposal adds (always with a "+" too). |
| `--warn` / `--warn-soft` | ochre | ochre | Something to fix (always with words too). |

Rules:

- **Never colour alone.** A change is green *and* starts with "+"; a warning is ochre *and* says what to do.
- **The red never means danger or deletion.** Refusing, removing and errors are said in words, in `--text`.
- **No gradients**, except the thread fading out under an empty screen.
- **No new colour** for a new feature: kinds of files, AIs and categories are told apart by icon and word, never by a colour code.

## 5. Shape and depth

| Token | Value | Use |
| --- | --- | --- |
| `--radius-s` | 6 px | Small marks: bars, keyboard hints, tiny tags. |
| `--radius` | 12 px | Fields, cards, banners, navigation items. |
| `--radius-l` | 18 px | Dialogs and the phone's bottom sheet. |
| `--radius-pill` | 999 px | Buttons, chips, segmented controls, counts. |

- Borders are 1.5 px for controls, 1 px for separators.
- **Only what floats casts a shadow**: menus and popovers (`--shadow-pop`), dialogs (`--shadow-dialog`). Cards and the page are flat.
- No blur, no glass, no glow.

## 6. Components

- **Buttons** are pills. *Primary*: filled `--thread`, white label, one per screen at most. *Secondary*: transparent, `--line-strong` border, `--text` label; the border turns red on hover. A disabled button is the same button at half opacity.
- **People** are filled discs with their initials (`.face`). **AIs** are open rings with their initial (`.ai-mark`): a person is a knot that is tied, an AI is one that is not. No robot, sparkle or brain icon stands for an AI.
- **Kinds of files** are an index (a ruled list with the count in display figures), not a grid of tiles.
- **A page** starts with its title (h1) and, if needed, one or two sentences of introduction (`.intro`, muted, `--step-1`). A status that needs attention (`.lead`) is bold, `--step-2`.
- **Icons** are 24 × 24 strokes, 1.8 wide, round ends, `currentColor` (`ui/src/lib/icons.ts`). One icon per idea; no filled icons, no emoji.

## 7. Motion

Owned by issue #169. Tokens ready for it: `--motion-quick` (150 ms) and `--motion-ease`. Something moves only when it appears, leaves or changes place; nothing moves on its own; everything stops with `prefers-reduced-motion`.

## 8. Voice

Ipse speaks like a careful friend who keeps your notebook: plain, short, concrete.

- Say what happens, then what to do: "Rien n'est écrit dans votre mémoire tant que vous n'avez pas accepté."
- No technical word (decision 0007), no exclamation marks, no emoji, no superlatives ("puissant", "révolutionnaire", "intelligent", "seamless").
- "Votre IA" and "votre mémoire": the person owns both.
- Numbers are written as numbers ("6 propositions"), dates in full ("Lundi 23 novembre").

## 9. What Ipse does not look like

Patterns that make an app look generated, and are not used:

- purple-to-blue gradients, gradient text, mesh or aurora backgrounds;
- glassmorphism, blurred panels, glows, neon outlines;
- a grid of identical rounded cards each with an icon in a tinted square;
- sparkles (✨), robots or brains for AI; emoji as icons;
- hero sentences with no content ("Unlock the power of your memory");
- tinted callout boxes for every remark (use a note on the thread);
- three accent colours where one would do.
