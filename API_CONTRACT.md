# Console API contract

All administration endpoints use `/api/`. Session cookie is `lxsh_session`, HttpOnly, SameSite=Strict. All requests include `X-Console-Path` equal to `location.pathname` (the security entry). Errors are `{ "error": "message" }`. JSON POST/PUT bodies use snake_case.

- GET `/api/bootstrap`: `{initialized:boolean, authenticated:boolean, username:string}`
- POST `/api/setup`, POST `/api/login`: `{username,password}` -> session cookie
- POST `/api/logout`
- GET `/api/groups`: array `{id,name,profile_ids:string[],default_strategy_enabled:boolean}`. New and legacy groups default the switch to false.
- POST `/api/groups`, PUT `/api/groups/:id`: `{name,profile_ids:string[],default_strategy_enabled?:boolean}`. Groups configure bundled selections and this optional default strategy, not arbitrary model lists or instructions. POST omission defaults false; PUT omission preserves its saved value; explicit false disables it. `null`, strings, numbers, arrays, and objects are rejected. Unknown profile IDs are rejected and duplicates deduplicated. `profile_ids:[]` removes all specific selections; it disables all injection only when `default_strategy_enabled` is false. Legacy unmodified presets without `profile_ids` migrate automatically; other legacy custom configuration does not become active.
- DELETE `/api/groups/:id` (reject referenced groups)
- GET `/api/accounts`: array `{id,name,endpoint,models:string[],model_mappings:Record<string,string>,group_ids,rpm,concurrency,priority,enabled,protocols,has_api_key,has_proxy,proxy_display,runtime:{active,failures,cooldown_until}}`. `models` contains selected actual upstream model IDs. `model_mappings` maps requested model IDs to actual upstream model IDs. Secrets omitted. Lower priority number wins.
- POST `/api/accounts`, PUT `/api/accounts/:id`: `{name,endpoint,api_key,models:string[],model_mappings:Record<string,string>,group_ids,protocols,rpm,concurrency,priority,enabled}`. Blank `api_key` on update retains the previous secret. `model_mappings` defaults to `{}` when omitted; every target must appear in the account's selected `models`. Each requested model is resolved once, with no recursive or chained mapping; absent a mapping, the requested model ID is used unchanged and must itself be selected in `models`. Model IDs must contain 1-256 bytes without whitespace or control characters. At most 500 selected models and 500 mappings are accepted. Select one or more protocols from `["responses","compact","chat","anthropic"]`; omitted protocols default to `["responses","compact","chat","anthropic"]`. `chat` routes Chat Completions requests to `/v1/chat/completions`; `anthropic` routes Messages requests to `/v1/messages`. Model names do not restrict protocol choice; the relay provider must support that model on the selected protocol. `rpm`/`concurrency` 0 means unlimited.
- Account create/update accepts optional `proxy_url`: omitted/null preserves saved proxy; empty string clears it; nonempty replaces it. Accepts SOCKS5/SOCKS5h with explicit nonzero port, optional username/password (1-255 decoded bytes each), no path except `/`, query or fragment. Whole URLs are encrypted and omitted from responses; `has_proxy` and credential-free `proxy_display` expose status.
- POST `/api/accounts/proxy-test`: `{proxy_url?,account_id?,endpoint?,api_key?,protocols?}` -> `{connected:boolean,ok:boolean,latency_ms:number,upstream_status:number|null,error:string|null}`. Auth required. Saved values can be reused; proxy and upstream endpoint required. Tests unsaved settings without saving, using GET models with 12-second total timeout and no redirects. HTTP rejection means connected but not ok; connection errors are sanitized. Validation errors use HTTP 400. Time includes proxy negotiation, TLS and upstream response headers, not ICMP ping.
- DELETE `/api/accounts/:id`
- POST `/api/models/discover`: `{endpoint,api_key,account_id?,protocols?:string[],proxy_url?:string|null}` -> `{models:string[]}`. Omitting `proxy_url` reuses the saved proxy, an empty string selects default transport, and a nonempty value uses an unsaved proxy. Unknown account IDs are rejected. Account ID enables reusing stored secret and, when protocols are omitted, stored protocols. Selecting `anthropic` uses `x-api-key` and `anthropic-version: 2023-06-01` for upstream discovery; otherwise Bearer authentication is used.
- GET `/api/keys`: array `{id,name,group_id,prefix,enabled,created_at,usage:{input_tokens,output_tokens,cached_tokens}}`
- POST `/api/keys`: `{name,group_id}` -> `{id,key}`
- PUT `/api/keys/:id`: `{name,group_id,enabled}`
- DELETE `/api/keys/:id`
- POST `/api/keys/:id/reveal`: `{key:string}`
- GET `/api/settings`: `{security_path,max_retries,failure_threshold,cooldown_seconds,request_timeout_seconds,retention_days}`
- PUT `/api/settings`: same object. Security path `""` or e.g. `"console-secret"`. After success frontend navigates to new path.
- POST `/api/password`: `{current_password,new_password,confirm_password}` -> all sessions revoked, return to login
- GET `/api/updates`: updater configuration, current version, last checked release and persisted job. A configured but unreachable controller returns an `error` field so reconnect polling can continue.
- GET `/api/updates/latest`: check the fixed official repository's latest non-draft, non-prerelease `vX.Y.Z` release.
- Release discovery works without the updater. Concurrent requests coalesce, successful release checks cache for 5 minutes, failures back off 1 minute; `/api/updates` retains the last known release after lookup failure. Authenticated visible console pages check every 15 minutes and synchronize automatic/manual state.
- POST `/api/updates`: strict `{tag,current_password}` starts an asynchronous source build of that latest release. Requires the current administrator password, with five attempts per minute. Wrong password returns 403 without invalidating the session; an active update returns 409. No user-supplied repository, URL, command, or arbitrary ref is accepted. Downgrades are rejected.
- Update response: `{current_version,configured,latest:{tag,version,url,published_at,notes}|null,update_available,job:{id,status,version,started_at,finished_at,message,logs:string[]}|null,error?}`. Times are Unix seconds. Statuses: `fetching`, `building`, `deploying`, `verifying`, `succeeded`, `failed`, `rolled_back`. A same-version source rebuild is supported. Poll `/api/updates` through a brief proxy restart; sessions and settings are persisted in the data volume.
- GET `/api/stats?from=unix_seconds&to=unix_seconds`: `{summary:{input_tokens,output_tokens,cached_tokens,uncached_tokens,total_tokens,cache_hit_rate,rpm,tpm,total_requests,success_rate,avg_latency_ms,cyber_count},daily:[{date,input_tokens,output_tokens,cached_tokens,total_requests,success_count,cyber_count}],recent:RequestLog[],accounts:{total,healthy,cooling},keys:number}`. Rates as percentages 0-100. Recent limited to 100. Dates UTC.
- GET `/api/profiles`: `{id,name,models:string[],instruction:string,model_family:string}[]` bundled specific-profile choices. There are 35 profiles for 36 exact IDs: three GPT, 14 Claude, one Grok, seven Gemini, five DeepSeek profiles covering six spellings, one GLM, and four Kimi. `coldcoffee-deepseek-v4.1-flash-v1` retains its persisted ID but loads Hanshuang text and matches both `deepseek-v4.1-flash` and `deepseek-4.1-flash`; other DeepSeek profiles are unchanged. `models` matches exact actual IDs after mapping; `model_family` is display metadata, not a wildcard. The default strategy is controlled separately and is not an extra catalog item.
- GET `/health`: `{status:"ok"}`

Dashboard token formatting is presentation-only: `K` at 1,000, `M` at 1,000,000, `B` at 1,000,000,000, `T` at 1,000,000,000,000, with two decimals for compact values. Smaller values remain integers. Statistics API token fields and raw exports retain original numeric values.

## Configuration Examples

Create a group with two independently selected profiles. Obtain current profile IDs from `/api/profiles`:

```json
{
  "name": "Primary",
  "profile_ids": ["gpt-5.6-sol-v45", "gpt-6-astra-v2-rc1"],
  "default_strategy_enabled": false
}
```

Create an account with selected actual models and a requested-model alias. Replace `GROUP_ID` with the created group's ID:

```json
{
  "name": "Primary upstream",
  "endpoint": "https://upstream.example/v1",
  "api_key": "YOUR_UPSTREAM_API_KEY",
  "models": ["gpt-5.6-sol", "gpt-6-astra"],
  "model_mappings": {"chat-main": "gpt-6-astra"},
  "group_ids": ["GROUP_ID"],
  "protocols": ["responses", "compact"],
  "rpm": 0,
  "concurrency": 0,
  "priority": 0,
  "enabled": true
}
```

## Public Routing

`profiles/default.md` adapts existing Astra task-execution and continuation mechanisms for multiple models and actual API capabilities. Its 3,844 UTF-8 bytes have provenance in `profiles/default.SOURCE.json`. Fixed model identity, local-runtime assumptions, and mandatory four-artifact output are removed. Client context remains intact; the profile does not claim to override upstream instruction hierarchy.

GET `/v1/models`, POST `/v1/responses`, POST `/v1/responses/compact`, POST `/v1/chat/completions`, and POST `/v1/messages` use a generated proxy key as Bearer or `x-api-key` authentication. The model list includes selected actual models and valid requested-model aliases from enabled accounts in the key's group. The upstream endpoint may include `/v1` or omit it. Streaming passes raw SSE. The proxy forwards each API in its original protocol without conversion between Responses, Chat Completions, and Anthropic Messages.

`/v1/chat/completions` selects accounts with the canonical `chat` protocol ID and uses Bearer authentication upstream. It preserves the Chat Completions request/response shape, including existing messages, tool fields, and multimodal content. The strategy selected by the specific-first/default-fallback rules is prepended as a separate `system` message; existing messages and other request fields are retained. JSON usage uses `prompt_tokens`, `completion_tokens`, and prompt cache details. SSE usage and the `[DONE]` completion marker are observed without converting chunks to Responses or Anthropic events.

`/v1/messages` selects accounts with the `anthropic` protocol and preserves the native Anthropic request/response shape, including tools and content blocks. Upstream authentication uses the account's secret as `x-api-key`; `anthropic-version` is forwarded (default `2023-06-01`) and `anthropic-beta` is preserved. The strategy selected by the same group rules is prepended to `system`, preserving existing strings or text block arrays and their cache controls. Anthropic streaming recognizes `message_stop`, errors, and truncated streams. Usage includes cache read and cache creation tokens in total input; cached tokens count cache reads. It uses the same model mapping, per-account failover, limits, and logging as the Responses routes.

All supported request protocols resolve the original client model independently per eligible account. Group, protocol, and the mapped actual model selected in account `models` must match. The relay sets `model` to that actual ID, prefers its selected specific profile, and otherwise applies the default only when `default_strategy_enabled` is true. With neither condition, no strategy is injected. At most one profile applies; specific and default text never stack. The default covers unmatched eligible models inside or outside the catalog and never expands account model/protocol/group eligibility. Responses and Compact prepend `instructions`; Chat Completions prepends a separate `system` message; Anthropic prepends `system` text. Existing client instructions, messages, tools, multimodal content, and other fields are preserved. Every failover starts from the original client request and recomputes mapping/strategy, without carrying or stacking previous-account text.

With the configuration above, `{"model":"chat-main","input":"Hello"}` sends `model: "gpt-6-astra"` with only the selected `gpt-6-astra` profile. An unmapped `gpt-5.6-sol` request keeps that model ID and uses only its own selected profile. An unmapped, unselected model makes this account ineligible. The matching rules are identical across all supported protocols; the example account enables only Responses and Compact.

The 27 profiles retaining coldcoffee text are inline Claude, Grok, Gemini, DeepSeek, and GLM configuration, including adapted Skill text; they do not install or execute local Skills/tools. Each match adds approximately 64–66 KB UTF-8. DeepSeek 4.1 Flash now uses the 4,748-byte Hanshuang API adaptation from `aimeoa/hanshuang-codex` commit `cf9c1537c2bd4ed8577ed6509ee665a39709d7d4`, with both spellings sharing the retained profile ID. Only general task-execution mechanisms from the shared prompt are adapted. `profiles/hanshuang/SOURCE.json` and `third_party/hanshuang/NOTICE.md` record source selection, hashes, and the upstream MIT README declaration with missing root LICENSE. `python scripts/import-hanshuang.py --check` verifies offline. Availability/forwarding do not guarantee model availability, behavior changes, upstream system-instruction precedence, or refusal bypass. READMEs document attribution and license facts.

## Administrators

Administrators are stored as Argon2-hashed accounts with a role of `owner` or `admin`. Sessions are bound to an administrator; legacy single-admin installs migrate to one `owner` entry on startup. User avatars show the first letter of the current administrator's username.

- GET `/api/me`: `{id,username,role,created_at}` for the current session.
- GET `/api/admins`: array `{id,username,role,created_at}`. Requires any administrator session.
- POST `/api/register`: `{username,password,role?}`. Before initialization it creates the first `owner` and returns a session cookie. After initialization it requires an `owner` session, or an `X-Register-Token` header equal to the `REGISTER_TOKEN` environment variable; new accounts default to `admin` unless `role:"owner"` is supplied.
- POST `/api/admins`: `{username,password,role?}` -> created administrator. Owner only. `role` is `owner` or `admin` (default `admin`).
- DELETE `/api/admins/:id`: owner only. Rejects deleting yourself or the last owner.
- POST `/api/admins/:id/password`: `{new_password}`. Owner can reset any administrator; an `admin` can only reset their own.
- POST `/api/setup` remains the first-run initializer and now creates the first `owner`. POST `/api/login` accepts any administrator username/password.
- POST `/api/password`: changes the current administrator's password; all sessions are revoked.

Validation mirrors the existing password policy (12-256 characters). Registration and administrator creation responses never include password hashes.

## Agent model routing

- GET `/api/agent/models`: array of `{provider, models:[...]}` groups. `provider` is the account name (or endpoint host when unnamed); models are the requested IDs from enabled accounts that include the `chat` protocol, derived from each account's `models` and `model_mappings` keys.
- PUT `/api/agent/config`: `{endpoint?,api_key?,model,system_prompt,allow_root,max_steps}`. When `endpoint` is empty (recommended), the agent resolves the account that supports `model` and calls that account's upstream directly, reusing its stored API key, model mapping and proxy. A nonempty `endpoint` remains available as a manual override.
- POST `/api/agent/chat` streams SSE events (`delta`, `tool`, `tool_result`, `error`, `done`) and no longer requires a separate provider key when using account routing.

## Agent conversations

- GET `/api/agent/conversations`: array `{id,title,model,updated_at,message_count}`, newest first, at most 100.
- GET `/api/agent/conversations/:id`: `{id,title,model,messages:[{role,content}],updated_at}`.
- PUT `/api/agent/conversations/:id`: `{title,model,messages}` upserts the conversation; `id` matches `[A-Za-z0-9_-]{1,100}`.
- DELETE `/api/agent/conversations/:id`.
