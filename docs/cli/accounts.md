# accounts — Manage accounts, sessions and client keys

Accounts share the music catalog and keep favourites, ratings, notes, history and collections private to their stable owner. The first administrator retains the existing `local` owner's data. Renaming or disabling a login keeps its personal data. The CLI remains trusted as the operating-system owner; its existing personal commands still address `local`.

## Syntax and arguments

```text
aede accounts [list]
aede accounts init <name> [--password-stdin]
aede accounts create <name> <admin|user|auditor> [--password-stdin]
aede accounts password <name> [--password-stdin]
aede accounts role <name> <admin|user|auditor>
aede accounts rename <name> <new-name>
aede accounts enable <name>
aede accounts disable <name>
aede accounts revoke <name>
aede accounts keys <name>
aede accounts keys <name> create <label>
aede accounts keys <name> revoke <key-id>
```

`init` creates the first administrator once. `create` requires an explicit role. `admin` manages accounts and installation work; `user` can change only their own personal data; `auditor` can read the shared catalog and their own personal API views only. An auditor cannot change their password through the API, edit personal data, manage accounts or run jobs. At least one administrator must remain enabled. `password` resets a password; `revoke` invalidates sessions and API keys without changing it. Changes to a login, role, password or enabled state revoke its sessions and keys. There is no account deletion or public registration; disabling preserves ownership for a later return.

Login names use 1–64 ASCII letters, digits, dots, underscores or hyphens and compare case-insensitively. At least one letter or digit is required. Passwords need at least 15 Unicode characters and at most 1024 UTF-8 bytes, without NUL. Leading and trailing spaces are preserved.

In a terminal, `init`, `create` and `password` ask for `Password:` and `Confirm password:`. Nothing is displayed while typing or pasting, including asterisks. Press Enter after each entry. Backspace removes the last Unicode character; Ctrl-U clears the entry. Ctrl-C, Ctrl-D or Escape cancel without saving. A mismatch or excessive length also leaves credentials unchanged. Terminal entry accepts printable characters; control characters are refused. Prompts use stderr, so `--json` output on stdout can be redirected. Standard input and stderr must both be terminals for masked entry. On macOS and Linux, `stty` must be available, as for local playback controls; Windows uses the native console. Pending pasted input is discarded while still masked, then the original terminal mode is restored before saving, including on cancellation or an input error. If cleanup fails, no credentials are saved. The error reports when echo remains disabled to protect pending input.

For scripts, explicitly use `--password-stdin` with redirected input. It accepts one line and removes a final LF or CRLF; embedded line breaks are refused. There is no confirmation for this mode. The flag refuses terminal input to avoid visible password entry. Passwords are never command-line arguments.

## Create the key before connecting a Subsonic client

**An Aède account password cannot connect Submariner or another Subsonic/OpenSubsonic client. Create an application key first:**

```sh
aede accounts keys alice create "Submariner Mac"
```

Replace the example name `alice` with your existing enabled administrator or user account. Use the same `--data` folder as the server. Copy only the complete value after `API key (shown once):`: **129 characters in `id.secret` form**, including the dot and both 64-character halves. For Submariner, paste it into **Password**; the table's ID alone is insufficient. The complete secret is shown only once and cannot be recovered by listing keys. Keep it privately or create a replacement if lost. See the [step-by-step Submariner setup](../server/subsonic.md), including executable paths and disabling token-based authentication.

To list keys or revoke one:

```sh
aede accounts keys alice
aede accounts keys alice revoke <key-id>
```

Listing displays metadata only; revocation removes only the selected key. Creation and revocation take effect without restarting the server. Keys survive restart, have no automatic expiry, and are removed by account changes, account-wide revocation and restore. Labels need non-whitespace text within 128 UTF-8 bytes. Maximum: eight keys per account, 512 total. `--json` returns metadata arrays for listing/revocation, or one metadata object plus `token` for creation. Keep that creation output private. Listing needs no writer lock; create/revoke use it.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--password-stdin` | Read one password line from redirected input for scripts using `init`, `create` or `password`, without confirmation. Omit the flag for masked terminal entry. |
| `--json` | Account operations output metadata arrays without passwords/verifiers/session tokens. Key operations use the shapes above; creation includes the one-time key secret. |

The shared [options reference](options.md) explains `--data`, colors and help. Listing needs no catalog or writer lock. Mutations use the data-directory lock directly, including when the server runs; account commands are never forwarded through command delegation. Password entry takes place before locking, so waiting at a prompt does not block other writers. The account store is then reloaded under the lock and all mutation rules are checked again before publication.

## Examples

From a terminal, enter and confirm each password when prompted:

```sh
aede accounts init operator
aede accounts create alice user
aede accounts password alice
```

For a script, with a protected one-line file already prepared at `/private/path/account-password`:

```sh
aede accounts init operator --password-stdin < /private/path/account-password
aede accounts create alice user --password-stdin < /private/path/account-password
aede accounts list --json
aede accounts rename alice listener
aede accounts revoke listener
aede accounts disable listener
```

Use distinct strong passwords. Keep the password file private and remove it when no longer needed. `accounts.json` stores salted Argon2id verifiers, never cleartext passwords. Under Unix its existing permissions must deny access to other users; correct an accidentally exposed file with `chmod 600` before retrying. Corrupt credentials are refused rather than recreated.

## Result and errors

An update publishes credentials atomically, without changing audio or `user.json`. Invalid names, short passwords, duplicate names, missing initialization and removing the last enabled administrator are refused. Backups include accounts; older backups and [reset](reset.md) preserve existing accounts. Restoring credentials invalidates all sessions.

Continue with [account sessions and HTTP administration](../server/accounts.md), [serve](serve.md), [backup](backup.md) and [restore](restore.md).
