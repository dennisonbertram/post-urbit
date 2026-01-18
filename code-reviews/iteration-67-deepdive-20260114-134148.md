## Iteration 67: DEEP DIVE

### Document Analysis
The spec is largely cohesive across Identity/Transport/Messaging/Runtime layers, with strong cross-references and test vectors. One remaining interoperability hazard is in the Mailbox protocol: the semantics of `message_id` returned by the mailbox (and used for deletion/ack) are not explicitly bound to the PUSE envelope’s `message_id` field, and the delete API references `message_ids` without normatively defining which identifier namespace those IDs belong to. Two reasonable independent implementations could choose different meanings and fail to interoperate (client cannot delete/ack messages on a mailbox implementation that chose the other meaning).

Everything else found in this pass is either:
- already disambiguated by the RFCs being authoritative over layer docs, or
- “MAY/SHOULD” policy divergence rather than a wire-level interop break, or
- internal API/interface consistency issues not affecting cross-implementation on-wire compatibility.

### Blocking Issues (B1, B2, etc.)

**B1 — Mailbox `message_id` namespace is ambiguous (interop break between mailbox servers and clients)**  
**Where:**
- `spec/06-rfcs/RFC-0003-messaging.md` §7.4.1 (Store) returns `"message_id": "<uuid>"`  
- `spec/06-rfcs/RFC-0003-messaging.md` §7.4.2 (Retrieve) returns objects with `"message_id": "<uuid>"`  
- `spec/06-rfcs/RFC-0003-messaging.md` §7.4.3 (Delete) takes `"message_ids": ["<uuid>", ...]`  
- `spec/00-shared/layer-integration.md` “MailboxService” returns `messageId: string` and deletes by that `messageId`, but does not define whether it equals the PUSE header `message_id`.

**Why this blocks interoperability:**  
A mailbox server can reasonably implement `message_id` as either:
1) **PUSE message identifier**: extracted from the envelope header `message_id` (UUID v4), or  
2) **Mailbox storage identifier**: a mailbox-assigned UUID (or other opaque ID) independent of the envelope’s header.

A client can reasonably implement deletion/ack as either:
- delete by the mailbox-returned `message_id` from retrieval responses, or
- delete by parsing the PUSE envelope and using the PUSE header `message_id` (common if client treats envelope `message_id` as the canonical message identifier everywhere).

If one side chooses (1) and the other chooses (2), the client’s DELETE calls can fail permanently (messages can’t be acknowledged, mailbox storage leaks, repeated redelivery, etc.). That is a direct cross-implementation interoperability failure.

**What to fix (normative): pick one of these and specify it clearly:**

Option A (simplest, recommended):  
- Mailbox **MUST** set mailbox `message_id` == PUSE header `message_id` (UUID v4), encoded as RFC4122 canonical lowercase string.  
- Mailbox **MUST** reject or treat as idempotent duplicate stores of the same `message_id` for the same recipient (define behavior).  
- DELETE `/messages` **MUST** interpret `message_ids[]` as these PUSE `message_id` values.

Option B (more flexible):  
- Define mailbox `message_id` as mailbox-assigned opaque ID, and add a separate field in retrieve responses (and optionally store responses) such as `puse_message_id` (UUID string) equal to the PUSE header.  
- DELETE must specify which ID it expects (mailbox ID vs PUSE ID), or provide separate endpoints/parameters.

Until one is chosen, independent mailbox server/client implementations cannot reliably interoperate.

### Minor Issues (M1, M2, etc.)

**M1 — Domain Separator Registry claims completeness but omits packaging-related separators**  
`spec/00-shared/layer-integration.md` “Domain Separator Registry (Normative)” says it lists *all* cryptographic domain separators, but `.postapp` signing uses strings like `postapp-signature-v1`, `postnode-repo-v1`, `postnode-update-v1` (in `spec/05-ux-packaging/app-distribution.md` and `spec/04-app-runtime/manifest-schema.md`).  
Not wire-breaking because those docs define the strings, but the registry’s “All … used across the Post-Urbit protocol” statement becomes misleading.

**M2 — Crockford Base32 normalization wording varies (reject vs normalize uppercase)**  
Some places say “reject uppercase,” others say “reject uppercase or normalize to lowercase.” Not a conforming-implementation interop break (senders are required to emit lowercase), but tightening language would reduce ambiguity for validators and error handling.

**M3 — Relay allocation binding wording is slightly inconsistent**  
RFC-0002 describes “UDP binding pending” then first UDP establishes binding; another section says “bound at creation time.” Not interop-breaking (procedural), but worth tightening to avoid divergent relay implementations.

### Verdict
[ ] SPEC-COMPLETE — No blocking issues remain  
[x] CONTINUE — 1 blocking issues require fixes

The spec is very close; resolving **B1 (Mailbox `message_id` semantics)** is necessary to ensure mailbox clients and mailbox servers from different implementations can reliably interoperate for store/retrieve/delete flows. Once that identifier namespace is explicitly defined, I don’t see remaining wire-level blockers.
