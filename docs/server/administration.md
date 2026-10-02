# Enable local administration

Administration can scan/fetch and access the local owner's favourites, ratings, notes, history and smart collections. It is optional and **disabled by default**. It is not an account system or permission to expose the server remotely.

## Configure the token before startup

Choose a private random ASCII secret of at least 32 characters, put it in `AEDE_ADMIN_TOKEN` in the server's environment, then start Aède. Use a protected service-manager secret when running a service. For a temporary terminal session, you can enter the secret without displaying it or placing its literal value in shell history:

```sh
read -r -s AEDE_ADMIN_TOKEN
export AEDE_ADMIN_TOKEN
aede serve
```

Type the secret and press Enter when `read` waits. This example assumes a shell supporting silent `read`, such as bash/zsh. Keep that terminal's environment private. An absent token leaves administrative routes unregistered (404); an invalid configured token prevents valid startup configuration. Setting the variable only in a client does not enable administration in an already running server: restart with the server environment configured.

In another trusted local client terminal, set the same environment variable securely. Each administrative request sends **one** header:

```sh
curl 'http://127.0.0.1:8787/api/admin/v1/collections' \
  -H "Authorization: Bearer $AEDE_ADMIN_TOKEN"
```

The literal value is expanded locally. Never place it in a URL, website JavaScript, repository, log screenshot or public command example. Any Origin header is refused, even one from the same localhost page. These are native/local-client routes, not a browser form backend. Foreign Host/Origin checks can reject a request before authentication.

## Request-body rules

For JSON, send `Content-Type: application/json` and an object, not an array/string. Supported field names use `snake_case`; unknown fields and wrong types give `400 invalid_body`. Bodies are limited to 16 KiB and must finish within one second (`408 request_timeout` on timeout). Valid JSON with invalid combinations/ranges gives `400 invalid_parameters`. Bodyless operations must not contain a JSON object. In particular, **no scan body** and **`{}` scan body** intentionally select different modes.

Reads use no body. Task routes accept no query parameters; personal routes only accept their documented selectors/pagination. GET personal/task routes also support HEAD with the same authentication and validation and no response body.

## Errors and permissions

`401 unauthorized` means the Bearer header is missing/incorrect/duplicated or an Origin was sent. `404 not_found` on all administrative routes usually means administration was not enabled at startup. A token authorizes filesystem access with the **server account's** permissions: scan folders and fetch targets are server-side paths. There is no sandbox for an untrusted caller.

The server reads music and tags without changing them. Image/lyric downloads and analysis sidecars are separate derivative files and may require destination write access. Source service keys come from the server environment, never HTTP overrides. Personal operations address the one existing `local` owner; a request cannot choose another owner.

Continue with [Scan/fetch jobs and cancellation](jobs.md) or [Personal-data routes](personal.md). The [operating guide](../operating.md) covers long-running service permissions and backups.
