# Security

Ipse holds a lot about a person's life. Thank you for helping keep it safe.

## Reporting a flaw

Please **do not open a public issue**. Use **Report a vulnerability** in the [Security tab of the repository](https://github.com/Loren844/ipse/security): only the project's owner sees it. Say what an attacker can do, and how to reproduce it, with invented data only: never send real personal data, yours or anyone's.

You will get an answer within a week. Once a fix is released, the flaw is described in the [changelog](CHANGELOG.md), with your name if you wish.

## Supported versions

Before 1.0, only the latest release gets security fixes.

## What Ipse protects, and what it does not

[What Ipse protects, and what it cannot](docs/guide/threat-model.md) explains, in plain words, who can see what, what leaves the computer, and the risks that remain, such as text written by others in an imported invitation (prompt injection, [decision 0014](docs/decisions/0014-imported-data-is-untrusted.md)). Reports about these limits are welcome too.
