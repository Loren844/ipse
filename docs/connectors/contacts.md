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

Cards with no name are skipped and counted; the count is sent in a final `progress` message.

## Not yet read

vCard 2.1 quoted-printable and other legacy encodings, addresses, titles, phone normalization to E.164: see #13.
