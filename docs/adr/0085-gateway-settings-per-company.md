---
title: "ADR 0085: The gateway's settings, per company, with the secrets sealed"
status: accepted
date: 2026-11-10
tags: [adr, fiscal, phase-7, security, settings]
---

# ADR 0085: The gateway's settings, per company, with the secrets sealed

## Context

[ADR 0073](0073-fiscal-module-plan.md), [0075](0075-fiscal-gateway-port.md) and [0077](0077-choosing-a-fiscal-gateway.md) put the fiscal
gateway's configuration **in the environment of the server only**: the token of each environment, the issuing CNPJ (one per
installation), `FISCAL_GATEWAY`, and a webhook secret, "never stored or logged". That is safe and awkward: the person who owns a
company cannot set up their own emission without a person who owns the server. The user asked for these settings to be **edited in the
app, under Configurações, with the secrets kept in the database**. This ADR reverses the "environment only" rule on purpose and says what
keeps it safe.

## Decision

### The settings are the company's, not the installation's

The page sits under `/companies/{id}/…` and a company has its own gateway account. An installation-wide token editable by one company's
owner would let one tenant change what another tenant issues through, so **each company stores its own** gateway kind (`focus` or
`nfeio`), the token of each environment (Focus NFe) or the API key and the company ids of each environment (NFE.io), the NFE.io search
confirmation, and the secret of its webhook. The rule "a document never goes out under another company's token" becomes: **the stored
configuration is usable only when its issuer CNPJ is the CNPJ of the same company's fiscal settings**, and a built adapter is never shared
between companies. The environment's configuration (ADR 0075) stays as the **fallback** for a company that has none, so every existing
installation and test keeps working unchanged; **a company's own settings win**.

### What a company cannot set

The **base URL** of the gateway and the **hosts files may be fetched from** stay in the environment (`FOCUS_NFE_API_URL`,
`NFEIO_API_URL`, `NFEIO_FILE_HOSTS`, and the built-in hosts): a tenant that could point the server at an address of its own would make the
server send its tokens and read answers from wherever it likes (SSRF). The `fake` gateway is for development and is not selectable here.

### Who may read and write it

Only the **owner and the admin**: the routes (`/gateway-settings`) are mapped to the `members` area, which no other role reaches (not even
`financial`, who writes the fiscal area, and not the accountant). The role-walking test keeps it so.

### Secrets are sealed, and write-only everywhere

- **Sealed** with AES-256-GCM (`ring::aead`, already in the dependency tree through TLS), behind a port (`SecretCipher`, sibling of
  `SecretService`): a fresh random 96-bit nonce per sealing, **the company and the field bound as associated data** so a ciphertext cannot be
  moved to another company or another field. The key is `FISCAL_SECRETS_KEY` in the environment (64 hexadecimal characters = 32 bytes).
  **Without it nothing can be saved** (`422`) and a stored secret says it cannot be read. **Losing the key makes the stored secrets
  unrecoverable** and the person types them again. Rotating the key is not built.
- **Write-only**: the API never returns a secret, only whether each is set. A change says per secret `keep`, `set` (with the value) or
  `clear`. The audit trail records that a secret was set, changed or cleared, **never its value or any part of it**; `Debug` hides it; nothing
  logs it. A test reads the raw column and asserts it is not the plaintext, and another that no response, trail or `Debug` holds it.
- The environment's secrets are unchanged and still never stored.

### Edited by two people, read with a version

The settings carry a **version** (1 for the first write): a write names the version it read and is **refused with `409`** (`GatewaySettingsChanged`)
when the stored one has moved, so an admin working from an old read can neither undo a secret another just set nor clear it by keeping what
it read (the same pattern as `QuoteChanged`). A removal is checked the same way. **Reset** (`DELETE /gateway-settings`, the page's "Voltar ao
gateway do servidor") removes the company's own settings so the server's come back; the trail records what was cleared, never a value.

### A secret that cannot be read, and what a secret may be

A stored secret that does not open (the key changed or was lost) is a **`409` naming the field** ("type it again"), not a 500, and the page
says which are unreadable instead of showing them as configured. A secret is **printable ASCII with no space, 8 to 200 bytes**: invisible and
non-ASCII characters (a copy from a page often adds one) are refused at the door, it is a valid HTTP header and basic-auth value for both
gateways, and its sealed size (at most 229 bytes) always fits the column. A body of the wrong shape is answered with a **fixed message**,
never serde's, which quotes the value it found. The key is 64 hexadecimal digits only (no sign) and **not all zeros**.

### What step 2 must do with it

"A company's own settings win" means: **the stored settings are usable when they have what the kind needs for the environment in use**
(Focus NFe: the token of that environment; NFE.io: the key and the company id of it). A row that is not usable is refused with a clear `422`
naming what is missing (not silently replaced by the server's, and not used half-built); reset is the way back. **The NFE.io search
confirmation stays the operator's too**: a company's own flag alone does not allow a production issue, because it is what keeps a document
from being issued twice ([ADR 0079](0079-nfeio-adapter.md)); production needs the environment's `NFEIO_SEARCH_CONFIRMED` as well.

### The webhook

A company may have its own webhook secret (sealed like the others). The knock is authenticated against the **company's** secret, else the
environment's. The company is looked up **before** the secret is checked, and an unknown company and a wrong secret are the same `401`, so the
route is no oracle for which companies exist ([ADR 0083](0083-received-invoices-and-the-fiscal-webhook.md)).

### In two steps

1. This ADR with the stored, sealed settings, the API and the page under Configurações (the settings are saved and shown; **no document
   uses them yet**).
2. The gateway that **resolves** a company's settings at the moment of a call (the capability questions take the issuer), with a test that
   one company's settings are never used for another's, and the webhook by company.

## Consequences

- A company can set up its gateway without touching the server. A database leak without the key does not leak the tokens; a leak with the
  key does, which is why the key lives only in the environment.
- Superseded: the parts of ADRs 0073, 0075 and 0077 that say the token is "in the environment only" and that there is "one issuing CNPJ per
  installation": both hold **for the fallback**, not for a company's own settings.
- **Not validated** here: that the sealed tokens work against a real gateway (nothing does yet, `docs/fiscal/homologation-test-plan.md`).

## What this does not decide

Rotating the key; per-user secrets; showing the last characters of a secret (not shown); the A1 certificate (still uploaded at the gateway).
