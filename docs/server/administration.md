# Enable local administration

Administration can scan/fetch and access the local owner's personal data. An [account administrator](accounts.md) may use their session. The optional legacy administrative token also works; without accounts or a token, administration is disabled. The listener stays loopback-only.

## Configure the token before startup

Choose a private random ASCII secret of at least 32 characters, put it in `AEDE_ADMIN_TOKEN` in the server's environment, then start Aède. Use a protected service-manager secret when running a service. For a temporary terminal session, you can enter the secret without displaying it or placing its literal value in shell history:

```sh
read -r -s AEDE_ADMIN_TOKEN
export AEDE_ADMIN_TOKEN
aede serve
```

Type the secret and press Enter when `read` waits. This example assumes bash/zsh and a private terminal environment. Without accounts, an absent token leaves administration unavailable (404); an invalid configured token prevents startup. Setting the variable only in a client does not change the running server: restart with its environment configured.

In another trusted local client terminal, set the same environment variable securely. Each administrative request sends **one** header:

```sh
curl 'http://127.0.0.1:8787/api/admin/v1/collections' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

The value is expanded locally. Never place it in a URL, website JavaScript, repository or log. The legacy token refuses every Origin header, even a matching localhost origin; account sessions follow the shared local-origin check. No browser login/cookie interface exists. Foreign Host/Origin checks can reject a request before authentication.

## Request-body rules

For JSON, send `Content-Type: application/json` and an object, not an array/string. Supported field names use `snake_case`; unknown fields and wrong types give `400 invalid_body`. Bodies are limited to 16 KiB and must finish within one second (`408 request_timeout` on timeout). Valid JSON with invalid combinations/ranges gives `400 invalid_parameters`. Bodyless operations must not contain a JSON object. In particular, **no scan body** and **`{}` scan body** intentionally select different modes.

Reads use no body. Task routes accept no query parameters; personal routes only accept their documented selectors/pagination. GET personal/task routes also support HEAD with the same authentication and validation and no response body.

## Errors and permissions

`401 unauthorized` means a missing/incorrect/duplicate Bearer header, or an Origin with the legacy token. A `user` or `auditor` account receives `403 forbidden`. Without accounts or a token, administration returns `404 not_found`. Jobs use the server's OS permissions: scan/fetch paths belong to that host and require a trusted administrator.

The server reads music and tags without changing them. Images, lyrics and analysis sidecars are derivatives that may require destination write access. Service keys come from the server environment, never HTTP overrides. Administrative personal routes retain `local`; `/api/me/v1` uses the session's owner. Requests cannot choose a different owner.

Continue with [Scan/fetch jobs and cancellation](jobs.md) or [Personal-data routes](personal.md). The [operating guide](../operating.md) covers long-running service permissions and backups.
