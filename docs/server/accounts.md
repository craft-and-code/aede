# Accounts, sessions and private data

Initialize accounts with [accounts](../cli/accounts.md). With no account store, catalog reads remain anonymous and local administration uses the optional `AEDE_ADMIN_TOKEN`. Once configured, every `/api/v1` HTTP or WebSocket request needs an account session or the administrative token. Missing, unreadable or exposed credentials fail closed. The server still listens only on loopback; accounts do not provide encrypted remote access.

An `admin` may manage accounts and installation work. A `user` may read and change only their own personal data. An `auditor` can read the shared catalog and use `GET` or `HEAD` only on their own `/api/me/v1` data; every personal mutation, password change, administration or job route returns `403 forbidden`. Auditors may still sign in, inspect their session and sign out.

## Sign in and manage a session

| Method and path | Body | Result |
| --- | --- | --- |
| `POST /api/auth/v1/session` | `{"username":"alice","password":"…"}` | 200 `{token,token_type:"Bearer",expires_at,account}` |
| `GET /api/auth/v1/session` | None | 200 account metadata |
| `DELETE /api/auth/v1/session` | None | 204; revoke this session |
| `PUT /api/auth/v1/password` | `{"current_password":"…","new_password":"…"}` | 204; revoke all sessions for this account (`auditor`: 403) |

`expires_at` is the absolute expiry in Unix seconds. Send exactly one `Authorization: Bearer <token>` header on subsequent requests. Query tokens, cookies and the administrative token cannot authenticate `/api/auth/v1` or `/api/me/v1`. Never put secrets in URLs, command arguments, logs or browser local storage. A protected JSON file can provide the login body with `curl --data-binary @/private/path/login.json`; secure its output because it contains the session token. This API targets explicit local clients; there is no browser login screen or cookie session yet.

Session tokens contain 256 bits from the OS random generator and stay in process memory. They expire after 12 hours or 30 minutes without an authenticated client request. Server notifications do not refresh inactivity. Logout, restart, credential/role/name/status changes and revocation invalidate sessions; credential restore rotates the store epoch. WebSockets recheck before sending and every second, then close invalid sessions. There are at most 256 sessions overall and eight per account; opening a ninth replaces that account's oldest session. Overall capacity returns `429 session_limit`.

Login/password verification uses two bounded workers. Limits are five attempts per login name and 100 overall per minute, including successful attempts; `429 login_limited` includes `Retry-After: 60`. Unknown names, disabled accounts and wrong passwords return the same `401 invalid_credentials`. Busy authentication returns `503 authentication_busy`; unreadable credentials return `503 accounts_unavailable`. Requests use the existing 16 KiB body limit and one-second body timeout, require JSON objects and reject unknown fields/query parameters. Responses carry `Cache-Control: no-store`.

## Own annotations, history and collections

Use the [personal routes](personal.md) with `/api/me/v1` replacing `/api/admin/v1`. Their selectors, bodies, pagination and response shapes are unchanged. The server derives the owner from the session; no request can choose it. This includes history counts and collection contents. A `user` or `admin` can use every listed method for that owner. An `auditor` can use only `GET` and `HEAD`; mutations return `403 forbidden`. Two workers shared with catalog inspection bound personal operations; contention returns `503 personal_busy` or `409 store_busy`. Every personal operation uses the current disk catalog and user store under the writer lock, then rechecks the session before exposing or changing data.

| Route | Methods |
| --- | --- |
| `/api/me/v1/annotation` | GET/HEAD/PUT by `ref` |
| `/api/me/v1/history` | GET/HEAD paginated events; POST a listen |
| `/api/me/v1/collection` | GET/HEAD/PUT/DELETE by `name` |
| `/api/me/v1/collections` | GET/HEAD paginated collections |

The transitional `/api/admin/v1/{annotation,history,collection,collections}` routes continue to address `local`, even when an administrator has a different owner. The first administrator inherits `local`. The CLI remains trusted as the OS owner and continues its existing local scope.

## Account administration

Administrators can run [installation jobs](jobs.md) and manage accounts; a `user` or `auditor` receives `403 forbidden` on administration. An enabled administrator must always remain. Use an administrator session or the optional legacy administrative token:

| Method and path | Body | Result |
| --- | --- | --- |
| `GET /api/admin/v1/accounts` | None | 200 array of account metadata |
| `POST /api/admin/v1/accounts` | `{username,password,role}`; `role` is `admin`, `user` or `auditor` | 201 new account metadata |
| `GET /api/admin/v1/accounts/{username}` | None | 200 account metadata |
| `PATCH /api/admin/v1/accounts/{username}` | At least one of `username,role,enabled,password` | 200 updated metadata; relevant sessions revoked |
| `DELETE /api/admin/v1/accounts/{username}/sessions` | None | 204; revoke all sessions for that account |

Metadata fields are `id,username,role,enabled,created_at,updated_at`. They contain no verifier, session generation or secret. PATCH rejects null, unknown fields and invalid changes without partial publication. Invalid account operations return `400 account_refused`; missing accounts return `404 account_not_found`. Initial setup/recovery stays with the local CLI; no public registration, account deletion or email recovery is implemented. See [the account design](../design/accounts.md) for protected storage and backup compatibility.
