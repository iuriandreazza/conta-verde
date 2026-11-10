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

### How a call finds its gateway (step 2)

The use case asks a **`FiscalGatewayResolver`** (a port in `application`) for the gateway **of the issuer in the settings it just
loaded**, at the moment of the call: `ready_settings` and `settings_of` return the settings **and the gateway resolved for them**, and
every call of the use case (the capability questions `issues_nfse`/`issues_nfce`/`issues_manifestation`, `submit_*`, `query_*`,
`cancel_*`, the events, the received invoices) goes through that one handle. "The capability questions take the issuer" is therefore met
by asking the handle that was resolved for that issuer. A gateway that is the same for everybody (the server's, a fake in a test) is its
own resolver. **The pass that finishes what is waiting reaches every company and resolves per document**, never once per pass, so one
company's token can only go out in that company's calls (`contract::a_pass_over_many_companies_asks_each_through_its_own_gateway`; and
`company_fiscal_gateways::tests` records which token each call carried).

`CompanyFiscalGateways` (in `infrastructure`) reads the company's row on **every** call (no cache: a change or a reset is felt at once and a
token is never kept past the call that needed it; a client is built per call, which is cheap at this volume):

- **No row**: the server's gateway, with the CNPJ guard it has always had (another company's issue is `WrongIssuer`, nothing sent).
- **A row that does not open**: `409` `SecretUnreadable`, naming the field; never the server's gateway in its place.
- **A row that is not usable** (it lacks what the kind needs for the environment of the call: Focus NFe, the token of that environment;
  NFE.io, the key and the company id of it): a `422` naming what is missing. The settings are never half used and the server's are never
  put in their place (that would issue under another token); reset is the way back.
- **A usable row** builds the adapter with the company's secrets and the **issuer's own CNPJ** (the CNPJ guard passes trivially for a
  company's own token: the token is the company's). The address, the hosts a file may come from and the city codes are the **operator's**
  (`GatewayEndpoints`, from `FOCUS_NFE_API_URL`, `NFEIO_API_URL`, `NFEIO_FILE_HOSTS`), read whichever gateway the server itself uses.
- **The NFE.io search confirmation stays the operator's too**: production needs the company's flag **and** the environment's
  `NFEIO_SEARCH_CONFIRMED` (read on its own, even when `FISCAL_GATEWAY` is something else), because it is what keeps a document from being
  issued twice ([ADR 0079](0079-nfeio-adapter.md)).

### The webhook

A company may have its own webhook secret (sealed like the others). **A company that has one is knocked at with that one only**: the
operator's no longer opens it, and neither does another company's. A company with none is knocked at with the operator's
(`FISCAL_WEBHOOK_SECRET`); the header's name is the operator's in both cases. The company is looked up **before** the secret is compared:
a malformed id, a company that does not exist, a wrong secret and a secret that cannot be read all answer the same `401`, so the route is
no oracle for which companies exist or have a secret ([ADR 0083](0083-received-invoices-and-the-fiscal-webhook.md)); the door being shut
(no secret to compare with at all) is the same `401`. What is still `404`, for a knock that carries the right secret, is a company with no
fiscal settings. A storage failure is a `5xx`, so the gateway retries instead of reading it as a wrong secret. The body is still never read.

### Changing the gateway while documents wait, and the polls

- **The gateway account is not changed while a document waits.** A document sent through one gateway and then asked about through
  another gets "no such document", is rejected after the grace period and, sent again, could be issued twice. Saving settings that change
  the account (a first row, another kind, token, key or company id; not the webhook secret or the search flag) and resetting them are
  refused with `422` while the company has a document `processing` or in `contingency`
  (`FiscalDocumentService::ensure_gateway_may_change`). The person waits, or asks about it again until the authority answers. Not covered:
  a cancellation or correction waiting as a `sent` event, and a document the authority never answers (the person's refresh settles it).
- **A knock polls its own company** (`poll_company`, `pending(Some(company), ..)`): it no longer makes the server ask the gateways of
  the others. The queued events are left to the minute pass, which covers every company.
- **The pass over every company is fair**: it reads ten times its size and gives each company a turn in turn (oldest of each first), so a company whose gateway cannot answer cannot take every place of the minute pass.
- **`refresh` reads again** when another writer moved the document between its read and its write (up to three times).

### What this does not solve (known limits)

- **Error precedence.** Fiscal settings are read before the gateway is resolved, so a company with none gets "set the fiscal settings"
  even on a server with no gateway, where it used to get `Disabled` first.
- A client is built on every resolve and the row is read and opened every time: cheap at this volume, not measured.

### In two steps

1. The stored, sealed settings, the API and the page under Configurações (the settings are saved and shown).
2. The gateway that **resolves** a company's settings at the moment of a call, with a test that one company's settings are never used for
   another's, and the webhook by company. **Done.** Nothing here has been tried against a real gateway
   ([`docs/fiscal/homologation-test-plan.md`](../fiscal/homologation-test-plan.md)).

## Consequences

- A company can set up its gateway without touching the server. A database leak without the key does not leak the tokens; a leak with the
  key does, which is why the key lives only in the environment.
- Superseded: the parts of ADRs 0073, 0075 and 0077 that say the token is "in the environment only" and that there is "one issuing CNPJ per
  installation": both hold **for the fallback**, not for a company's own settings.
- **Not validated** here: that the sealed tokens work against a real gateway (nothing does yet, `docs/fiscal/homologation-test-plan.md`).

## What this does not decide

Rotating the key; per-user secrets; showing the last characters of a secret (not shown); the A1 certificate (still uploaded at the gateway).
