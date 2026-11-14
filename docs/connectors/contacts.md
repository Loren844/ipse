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
| `related` | list of `{name, uid, kind}` | vCard 4 `RELATED` (`uid` for a `urn:uuid:` pointing to another card, `name` for `VALUE=text`; other links and `TYPE=me` left out), and Apple's `X-ABRELATEDNAMES` with the `X-ABLabel` of the same group. `kind`: the vCard type (`co-worker` read as `colleague`), Apple's built-in labels as plain words (`_$!<Mother>!$_` is `mother`), or the label the person typed, as written. |

Cards with no name are skipped and counted; the count is sent in a final `progress` message.

## Related names on your own card

Address books often know who is who: on iPhone and Mac, your own card lists related people ("mother: Maman"). Ipse turns them into relations of your own file, in the vault's language: `- mère : [[maman]]`.

- Your card is the one card that has an email or a phone number that is also on your own file (`emails`, `phones`), compared on the last 9 digits for phones. Apple's exports do not mark the "me" card, and Ipse never guesses it from a name: add your email to your file first.
- A related person is linked only when exactly one person file has that name or other name, or comes from the card a `urn:uuid:` points to. Otherwise nothing is written.
- A relation to someone your file already links to is not added again, whatever its word.
- The person always wins: the relations Ipse added are listed in `.ipse/added-relations.toml`, so that one you remove is never added back.

## Not yet read

vCard 2.1 quoted-printable and other legacy encodings, addresses, titles, phone normalization to E.164: see #13.
