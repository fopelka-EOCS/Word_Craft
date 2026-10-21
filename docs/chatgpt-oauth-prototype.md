# ChatGPT OAuth local prototype (#815 / M25)

> **Last reviewed:** 2026-10-21 · **Last updated:** 2026-10-21 · **Change:** minor (credential lifecycle/callback fixes; macOS CI passed, final live/platform acceptance pending)

This branch adds a macOS-only ChatGPT connection to the #815 assistant. It uses WordCraft's
own dynamic open-source registration, the public Responses API, and macOS Keychain. It needs
neither an API key nor Codex/Hermes. Existing API-key providers retain their settings and transport.
The previous prototype completed live inference. Eligibility remains account-dependent and is
confirmed in the UI only after a completed request; prior live evidence does not validate a rebuilt executable.

## Run and use

Run `cargo run -p wordcraft -- --control 7983`. A locally built acceptance candidate can be
saved at `plan/chatgpt-oauth/wordcraft`; record its commit and SHA-256 before testing. Launch it with
`WORDCRAFT_NO_PREFS=1 ./plan/chatgpt-oauth/wordcraft --control 7983`. For an isolated preview that does not read or
write ordinary app/provider preferences, set `WORDCRAFT_NO_PREFS=1`; OAuth still uses its own
Keychain service so the remembered-login test works.

1. Open Home → Assistant → Settings, select ChatGPT, then Continue with ChatGPT.
2. Complete registration in the system browser. WordCraft opens its listener on `127.0.0.1`
   before opening the browser, and uses `/auth/callback` with the same selected port for exchange.
3. Review Manage usage and acknowledge the ChatGPT plan/credits explanation with Got it.
   Limits do not guarantee zero purchased-credit use. WordCraft never changes usage controls or
   switches billing methods/providers automatically.
4. Choose an account-provided model. The picker shows display names and sends their slugs.
5. Allow document access before Send. Consent covers the document's text, selection, messages
   and tool results for this document session. It is never saved to preferences or documents.
6. Ask for an edit. It follows #815's read-before-edit, permissions, selection scope, tracked
   changes, verification reminder and one-Undo-step safeguards.
7. Restart to test remembered login. Consent must be given again. Disconnect removes local
   tokens and attempts remote refresh-session revocation. If revocation cannot be confirmed,
   the app directs you to disconnect in ChatGPT settings.

New/Open (including Open delivered through the control channel), provider/account changes,
and Disconnect clear conversation context and consent, drop late results, and restore the
old document's temporary author/tracking state before replacing it. A failed Open also stops
pending assistant work conservatively. Save As keeps the current document session.

## Security and architecture

- `crates/assistant/src/chatgpt.rs`: sanitized connection interface, without tokens.
- `crates/assistant/src/responses.rs`: fixed public endpoint, `store: false`, `stream: true`,
  incremental SSE decoding. Only a completed response can enter the agent loop; malformed,
  unknown, duplicate or incomplete function calls fail the entire response. Encrypted reasoning
  and complete tool outputs are replayed for follow-up turns.
- `apps/wordcraft/src/chatgpt.rs`: system-browser PKCE (S256), random state and nonce, bounded
  loopback callback, five-minute authorization deadline, JWKS signature/issuer/audience/expiry/
  nonce verification, granted-scope checks, serialized refresh, and Keychain storage.
  Authorization and code exchange both specify the same public API resource.
- Keychain service: `ai.storyteller.wordcraft.chatgpt.local-prototype`. `registration` retains the
  opaque host ID and the single account/client mapping. `credentials` atomically binds that
  registration to its access/refresh/ID tokens and expiry. Disconnect deletes `credentials`;
  the registration remains for later sign-in. Storage failures are reported without plaintext
  fallback. OAuth credentials never enter `assistant.json`, `ui.json`, or document files.
- Disconnect cannot be cancelled or superseded while local deletion is pending. A failed Keychain
  deletion retains a connected state so the owner can retry; failed remote revocation is reported
  separately after local removal. The control interface exposes only a sanitized `disconnecting` flag.
- Callback input is read incrementally with cancellation, a total deadline and an 8 KiB request-line
  cap; fragmented browser requests are accepted even when macOS inherits nonblocking socket mode.
- A locked `chatgpt-prototype.lock` in the settings directory contains no secrets and prevents
  two prototype processes racing a rotating refresh token.
- Native worker errors are generic; echoed credentials are removed before delivery. OAuth worker
  logs, including dependency HTTP diagnostics, stay disabled even with `RUST_LOG=trace`.
- Cancellation immediately drops the agent's receiver and restores editing state. Native work
  checks cancellation before sending and between stream events. A blocked socket can remain
  active until the next event or the 180-second HTTP deadline; stopping cannot undo credit use
  already incurred. No late reply can be applied to another document.
- OAuth/Keychain dependencies and service injection are macOS-only. Other native builds and
  WASM retain the existing providers; they show ChatGPT as unavailable rather than using a
  different authentication or billing method.

## Historical live acceptance and final-build gate

Automated tests cover mocked token/JWKS endpoints, verified sign-in, invalid state, duplicate
callback fields, changed client ID, cancellation, timeout, invalid ID-token signature/claims,
missing plan permission, token refresh/rotation/failure, secure-storage failure, credential
removal and restart. Wire tests cover text, tools, reasoning replay, incomplete/interrupted
streams, quota failures and token redaction. UI tests cover consent gates, tracked edits,
Undo/rejection, New/Open, account changes, disconnect and late replies.

The checklist below records historical acceptance on commit `44c6b4b4`, executable SHA-256
`930b194f6ed6a57a2f6695a6c1fe7fd2aa0c43727832d59a6e6acdc90d79d00d`. The owner completed browser sign-in,
usage acknowledgment and document consent. The preview was signed out after Disconnect. These
checks must be repeated on the final executable after the corrections below:

- [x] Browser registration and validated connection
- [x] Usage acknowledgment and document consent
- [x] Completed response confirms account eligibility; visible exact-response test passed
- [x] Synthetic document edit, tracked changes and one-step Undo; original text restored
- [x] Restart, remembered login and fresh document consent (Keychain permission prompted for the rebuilt executable)
- [x] Disconnect completed without error; remote revocation confirmed and the credentials Keychain item was absent afterward (macOS `security` exit 44)
- [x] Application window and headless usage/consent screenshots visually inspected

Deferred: Claude OAuth, multiple saved accounts, ChatGPT history import, incremental display
of partial answers, non-macOS secure storage, upstream publication/submission. API-key storage
in #815's existing `assistant.json` is unchanged.

## Live stream correction and quality gates

The initial live test exposed a route-specific behavior that the first mock fixtures missed:
`response.output_item.done` contains the answer, while `response.completed.output` is empty.
The adapter now retains complete indexed items and reconstructs terminal output only after
successful completion. A regression test failed before the fix and passed afterward; a tiny
live request then returned its five-character answer rather than being silently dropped.
Empty completed replies now report an error. Live editing then correctly read, planned, selected,
replaced and verified the synthetic word change. The Undo check also exposed #815 recording the
assistant's temporary tracking flag in its first Undo snapshot. The harness now checkpoints the
original tracking preference before its first mutation, joining later edits as before; failed or
refused edits restore the whole snapshot. The new regression failed before this correction and
passed afterward, including Redo preference preservation. The request also namespaces WordCraft tools and
omits unsupported `max_output_tokens`, following the preview limitations.

`cargo xtask ci` passed all six stages on macOS: 1,531 tests passed, zero failed, one existing
ignored translation-audit test, and all 17 WASM crate checks passed. The existing WASM-only
`speech.rs` dead-code warning is unchanged. Final local validation is recorded separately under ignored `plan/chatgpt-oauth/`: exact review
base/head, command logs, dependency/secret scan results, independent Standards/Spec reports and
executable checksum. The earlier full-CI attempt failed on unchanged #815 lint errors. This pass
reproduces those errors and keeps behavior-preserving lint cleanup separate from OAuth fixes,
without suppressing warnings or skipping tests. Undo/Redo mocks cover both original tracking values.

The credential-removal cancellation race and fragmented callback regression failed before their
corrections and passed afterward. Mock deletion failures remain retryable, and mock revocation
failures report local removal separately. TLS lockfile updates address six RustSec advisories;
three ZIP extraction advisories remain outside the APIs WordCraft calls. The scan is not described
as vulnerability-free. The embedded synthetic test signing key is explicitly classified as test
material, without a general key-detection suppression.

#815 remains an open upstream dependency. Windows/Linux native evidence and owner-driven acceptance
on the final executable are required before calling this ready for review. The prepared validation
workflow extends upstream's general CI at the same path with macOS/Windows/Linux runners, read-only
permissions, no account secrets and no release jobs; native OAuth tests compile only on macOS.
Prior screenshots and live results must never be attached as evidence for a new executable.

macOS may ask for permission to read the prototype's Keychain entry after a rebuild or executable
identity change. A stable, properly signed release needs separate packaging and Keychain
acceptance testing; this prototype does not promise prompt-free access. The app never receives
the password entered into the macOS permission dialog. See
[Apple's Keychain permission guide](https://support.apple.com/en-gb/guide/mac-help/kychn002/26/mac/26)
and [Apple DTS on app identity across updates](https://developer.apple.com/forums/thread/115425).

## Official sources

Implementation checked against OpenAI documentation on 2026-10-21:
[registration/sign-in](https://developers.openai.com/siwc/token-sharing-open-source/sign-in),
[accounts, refresh and revocation](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions),
[models/inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference),
[usage UI](https://developers.openai.com/siwc/ui-ux-guidelines),
[preview limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations), and
[ID-token verification](https://developers.openai.com/siwc/website).

The local execution-plan/architecture files and shared `../../craftrules` checkout were unavailable. The referenced GitHub
craftrules repository was unavailable (404); the supplied WordCraft AGENTS.md was followed.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-21 | trivial | Authorization now supplies the documented API resource, pinned for initial and returning sign-in; browser invalid_state during final live acceptance remains under investigation |
| 2026-10-21 | minor | Final review: noncancellable, retryable Disconnect; bounded fragmented callbacks; mocked tracking-on/off Undo/Redo; final-build and platform gates recorded separately |
| 2026-10-21 | minor | Initial macOS OAuth/Keychain prototype on #815; historical owner live acceptance completed |
