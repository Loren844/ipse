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
- **Outlook**: export a contact as a vCard file.

## Records

One `upsert` per card. `external_id` is the card's `UID`; a card without one gets a hash of its content, so importing the same file twice gives the same ids (a card without `UID` whose content changes is seen as a new card).

`data`, fields left out when empty:

| Field | Type | From |
| --- | --- | --- |
| `full_name` | text | `FN`, else given and family names from `N` |
| `given_name`, `family_name` | text | `N` |
| `nicknames` | list of text | `NICKNAME` |
| `emails` | list of text | `EMAIL` |
| `phones` | list of text | `TEL` (a `tel:` prefix is removed) |
| `birthday` | `YYYY-MM-DD` or `--MM-DD` | `BDAY`; Apple's stand-in year (`X-APPLE-OMIT-YEAR`) is dropped |
| `organization` | text | `ORG`, parts joined with `, ` |
| `note` | text | `NOTE` |
| `related` | list of `{name, uid, kind}` | vCard 4 `RELATED` (`uid` for a `urn:uuid:` pointing to another card, `name` for `VALUE=text`; other links and `TYPE=me` left out), and Apple's `X-ABRELATEDNAMES` with the `X-ABLabel` of the same group. `kind`: the vCard type (`co-worker` read as `colleague`), Apple's and Google's built-in labels as plain words (`_$!<Mother>!$_` is `mother`, `DOMESTIC_PARTNER` is `domestic partner`), or the label the person typed, as written. |

Cards with no name are skipped and counted; the count is sent in a final `progress` message.

## Your own card

Your address book usually has a card for you. Ipse recognizes it as the card linked to your own file, else the one card that has an email or a phone number that is also on your own file (`emails`, `phones`; phones compared on their last 9 digits). Apple's and Google's exports do not mark the "me" card, and Ipse never guesses it from a name: add your email to your file first. Two cards that match: none is used.

**It completes your file instead of making a second one.** Your file is yours: Ipse only adds what the card brought since it last read it (emails, phones, the card's name and nicknames as other names, a birthday your file lacks), never replaces or removes anything, and links your file to the card (`sources`). Something you remove from your file is not added back. A card that an earlier import already gave its own file keeps it; delete that file and import again to have your card complete yours.

**Related names become your relations.** On iPhone, Mac and in Google Contacts, your card lists related people ("mother: Maman"). Ipse writes them in your file, in the vault's language: `- mère : [[maman]]`.

- Apple and Google both write them as `X-ABRELATEDNAMES` with the `X-ABLabel` of the same group. Built-in labels are `_$!<Mother>!$_`; Google also writes a few in capitals (`RELATIVE`, `DOMESTIC_PARTNER`, `REFERRED_BY`). A label you typed is kept as written. vCard 4 `RELATED` is read too, including a `urn:uuid:` pointing to another card.
- A related person is linked only when exactly one person file has that name or other name, or comes from the card a `urn:uuid:` points to. Otherwise nothing is written.
- A relation to someone your file already links to is not added again, whatever its word.
- The relations Ipse added are listed in `.ipse/added-relations.toml`, so that one you remove is never added back.

## Not yet read

vCard 2.1 quoted-printable and other legacy encodings, addresses, titles, phone normalization to E.164: see #13.
