# Contacts connector

Imports the people you know from a contacts file (`.vcf`).

| | |
| --- | --- |
| Id | `contacts` |
| Rung (decision 0008) | 4, file export: every contacts app exports vCard files; no account or network access is needed. |
| Modes | `import` |
| Access | the files the person picks; no network; no secret |
| Category | `people` |
| Kind | `contact.card` |

## Getting the file

- **Google Contacts**: contacts.google.com → Export → vCard.
- **Apple Contacts**: select the contacts → File → Export → Export vCard.
- **Outlook**: export a contact as a vCard file. Outlook writes vCard 2.1, often in quoted-printable and Windows-1252: Ipse decodes both.

vCard 2.1, 3.0 and 4.0 are read. Folded lines, quoted-printable values (with their soft line breaks) and the `CHARSET` of a value are decoded; a line that is not UTF-8 is read as Windows-1252. Photos and other binary values are left out.

## Records

One `upsert` per card. `external_id` is the card's `UID`; a card without one gets a hash of its content, so importing the same file twice gives the same ids (a card without `UID` whose content changes is seen as a new card).

`data`, fields left out when empty:

| Field | Type | From |
| --- | --- | --- |
| `full_name` | text | `FN`, else given and family names from `N` |
| `given_name`, `family_name` | text | `N` |
| `nicknames` | list of text | `NICKNAME` |
| `emails` | list of text | `EMAIL` |
| `phones` | list of text | `TEL`, in E.164 (`+33612345678`) when it can be read with certainty, else as written (see below) |
| `birthday` | `YYYY-MM-DD` or `--MM-DD` | `BDAY`; Apple's stand-in year (`X-APPLE-OMIT-YEAR`) is dropped |
| `organization` | text | `ORG`, parts joined with `, ` |
| `title` | text | `TITLE` |
| `addresses` | list of text | `ADR`, each on one line: extended address, street, postal code and town, region, country (`12 Rue des Lilas, 69003 Lyon, France`) |
| `note` | text | `NOTE` |
| `related` | list of `{name, uid, kind}` | vCard 4 `RELATED` (`uid` for a `urn:uuid:` pointing to another card, `name` for `VALUE=text`; other links and `TYPE=me` left out), and Apple's `X-ABRELATEDNAMES` with the `X-ABLabel` of the same group. `kind`: the vCard type (`co-worker` read as `colleague`), Apple's and Google's built-in labels as plain words (`_$!<Mother>!$_` is `mother`, `DOMESTIC_PARTNER` is `domestic partner`), or the label the person typed, as written. |

Cards with no name are skipped and counted; the count is sent in a final `progress` message.

## Phone numbers

- A number written with its country code (`+33 6…`, `0033 6…`, `+33 (0)6…`) is always written in E.164.
- A national number (`06 12 34 56 78`) is read in the country of the vault's time zone (`Europe/Paris` is France), for the countries whose numbering Ipse knows: most of Western Europe, North America (`011` dialled abroad too) and some of North and West Africa. Its length is checked; Italian numbers keep their leading `0`.
- Anything else stays as written: letters or an extension (`poste 214`), a national number with no known country, a length that does not fit. A wrong number is worse than an unformatted one.

In the vault, a person's file gets the card's name, other names, emails, phones, birthday and **first address**, kept up to date key by key (`docs/spec/vault.md`, section 9). The organization and the job title become facts of a new file (`Organisation : …`, `Poste : …`).

## Your own card

Your address book usually has a card for you. Ipse recognizes it as the card linked to your own file, else the one card that has an email or a phone number that is also on your own file (`emails`, `phones`; phones compared on their last 9 digits). Apple's and Google's exports do not mark the "me" card, and Ipse never guesses it from a name: add your email to your file first. Two cards that match: none is used.

**It completes your file instead of making a second one.** Your file is yours: Ipse only adds what the card brought since it last read it (emails, phones, the card's name and nicknames as other names, a birthday or an address your file lacks), never replaces or removes anything, and links your file to the card (`sources`). Something you remove from your file is not added back. A card that an earlier import already gave its own file keeps it; delete that file and import again to have your card complete yours.

**Related names become your relations.** On iPhone, Mac and in Google Contacts, your card lists related people ("mother: Maman"). Ipse writes them in your file, in the vault's language: `- mère : [[maman]]`.

- Apple and Google both write them as `X-ABRELATEDNAMES` with the `X-ABLabel` of the same group. Built-in labels are `_$!<Mother>!$_`; Google also writes a few in capitals (`RELATIVE`, `DOMESTIC_PARTNER`, `REFERRED_BY`). A label you typed is kept as written. vCard 4 `RELATED` is read too, including a `urn:uuid:` pointing to another card.
- A related person is linked only when exactly one person file has that name or other name, or comes from the card a `urn:uuid:` points to. Otherwise nothing is written.
- A relation to someone your file already links to is not added again, whatever its word.
- The relations Ipse added are listed in `.ipse/added-relations.toml`, so that one you remove is never added back.

## Not yet read

Several job titles or roles (`ROLE`), the type of each address (home, work), photos.
