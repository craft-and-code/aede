# Aède accounts

`aede-accounts` owns stable personal identities, administrator/user/auditor roles, account administration, salted Argon2id password verification, credential generations and independently revocable client API keys. It has no dependency on the catalog, DSP, HTTP transport or `aede-core`.

The JSON document remains account-store format 1, including optional `api_keys`; the first administrator keeps the `local` owner. Integral legacy number spellings remain readable. Documents returned by `to_json` contain private salted verifiers: use them only for protected persistence or full backups, never account-list or catalog responses. Debug output and public account/key metadata omit verifiers and secrets.

The `aede-core::accounts` facade retains installation paths, bounded private-file reads, permissions/descriptor checks, the existing JSON parser, atomic replacement writes and backup integration. Read/modify/save callers hold the shared data-directory writer lock. HTTP session lifetime, admission/rate limits and bounded password workers remain in `aede-server`; this crate does not create a session or expose registration.

Dependencies reuse the approved Argon2 0.5.3 policy and the workspace's existing `serde_json`. No registry package is added by the extraction. The [account design](../../docs/design/accounts.md) is the behavioural contract.

