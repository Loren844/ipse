# If it does not work

- **A file of your memory seems ignored:** `ipse check --vault ~/Ipse` lists the files Ipse cannot read and what to fix in each, without changing anything.
- **Ipse is not among the connectors:** check that the JSON is valid and that both paths are full and correct. Try the command in a terminal: `ipse mcp --vault /Users/you/Ipse`. If the folder is wrong, it says so and stops; otherwise it waits for an AI app (stop it with Ctrl+C).
- **Claude says the access code is not valid:** the code was mistyped or the AI was removed. Run `ipse client add` again and paste the new code.
- **Ipse could not use the keychain:** on Linux, install and unlock a keyring (GNOME Keyring or KWallet); on macOS, allow `ipse` when the system asks.
- **A calendar link no longer works:** it was reset in your calendar app. Copy the new one, then `ipse calendar unsubscribe <id>` and `ipse calendar subscribe` again.
- **Logs:** Claude Desktop keeps what Ipse says in `mcp-server-ipse.log`, in `~/Library/Logs/Claude` on macOS and `%APPDATA%\Claude\logs` on Windows.
- **An import or a sync went wrong, or you want to undo one:** `ipse ui --vault ~/Ipse`, then **Historique** (**History**). It lists every change with who made it and lets you undo one (as a new change: nothing is lost). Under **Imports et synchros**, each run says what came in or what failed, and a connector's last diagnostic lines can be shown, to attach to a bug report.
