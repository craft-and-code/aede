# accounts — Manage accounts, sessions and client keys

Accounts share the music catalog and keep favourites, ratings, notes, history and collections private to their stable owner. The first administrator retains the existing `local` owner's data. Renaming or disabling a login keeps its personal data. The CLI remains trusted as the operating-system owner; its existing personal commands still address `local`.

## Syntax and arguments

```text
aede accounts [list]
aede accounts init <name> --password-stdin
aede accounts create <name> <admin|user|auditor> --password-stdin
aede accounts password <name> --password-stdin
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

`keys` lists a named account's persistent OpenSubsonic keys. `keys … create <label>` saves a new key and shows its full secret once; `keys … revoke <key-id>` removes only that key. Labels need non-whitespace text within 128 UTF-8 bytes. Maximum: eight keys per account, 512 total. `--json` returns metadata arrays for listing/revocation, or one metadata object plus `token` for creation. Keep that creation output private. Listing needs no writer lock; create/revoke use it. These keys survive restart, have no automatic expiry, and are removed by account changes, account-wide revocation and restore. See [client setup and supported methods](../server/subsonic.md).

Login names use 1–64 ASCII letters, digits, dots, underscores or hyphens and compare case-insensitively. At least one letter or digit is required. Passwords need at least 15 Unicode characters and at most 1024 UTF-8 bytes, without NUL. The stdin interface accepts one line and removes a final LF or CRLF; embedded line breaks are refused.

## Options for this command

| Option | Meaning |
| --- | --- |
| `--password-stdin` | Read the password from redirected input for `init`, `create` or `password`. Interactive terminal input is refused. Passwords are never positional arguments. |
| `--json` | Account operations output metadata arrays without passwords/verifiers/session tokens. Key operations use the shapes above; creation includes the one-time key secret. |

The shared [options reference](options.md) explains `--data`, colors and help. Listing needs no catalog or writer lock. Mutations use the data-directory lock directly, including when the server runs; account commands are never forwarded through command delegation.

## Examples

With a protected one-line file already prepared at `/private/path/account-password`:

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
