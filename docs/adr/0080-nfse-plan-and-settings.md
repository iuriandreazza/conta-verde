---
title: "ADR 0080: NFS-e: the plan and what the settings must say first"
status: accepted
date: 2026-11-10
tags: [adr, fiscal, phase-7, nfse]
---

# ADR 0080: NFS-e: the plan and what the settings must say first

## Context

[ADR 0073](0073-fiscal-module-plan.md) puts the NFS-e after the NF-e and says the municipal tax of a service is decided with it.
[ADR 0077](0077-choosing-a-fiscal-gateway.md) says every fiscal call is built for **both** gateways (Focus NFe and NFE.io) and the fake,
with the shared contract. The NFS-e is the first document that is not a copy of the NF-e: it is a service, taxed by the **municipality** (ISSQN),
and Brazil is moving every municipality to one national standard (the *NFS-e Nacional*, with the DPS, the *declaração de prestação de
serviço*, that the issuer sends and the system answers with the NFS-e).

What the product kept so far does not give an NFS-e what it needs:

- **Where it is issued.** The NFS-e names the municipality of the issuer by its **IBGE code** (seven digits). The settings keep the
  city's name and the state, which cannot tell two cities of the same name apart nor spell one the same way twice.
- **What the service is.** A service nature had no tax data at all (ADR 0074 left it to the NFS-e): no code from the national list of
  services, no way the ISSQN is charged, no rate.
- **What the gateway needs of a service.** Focus NFe's national endpoint and NFE.io's service-invoice API both ask for the national code
  of the service; a gateway whose layout is municipal also asks for the municipality's own code.

## Decision

### The work is cut in three pull requests

1. **5a, the settings** (this ADR): what the issuer and a service nature must say. No document yet.
2. **5b, the document**: an NFS-e kind in the document store, the numbering of the DPS, the port's NFS-e calls with the fake, Focus NFe
   and NFE.io (and the shared contract), issuing from a sale that has service lines, and the API.
3. **5c, the screens**: the Documentos page issues and follows NFS-e.

### Issuer: the municipality's IBGE code

`FiscalSettings` gets `city_ibge_code`: seven digits or nothing (domain, a `CHECK`, the API, the page, the trail). It is **optional while a
company only issues NF-e**, and an NFS-e refuses without it. The product does not guess it from the city: the code is the company's to
type, from the IBGE table, and only its shape is checked. (The recipient's code of an NF-e comes from BrasilAPI's list, ADR 0079, because the
buyer is not ours to ask; the issuer is.)

### Service: the ISSQN terms of a nature

A service nature may carry `ServiceTerms`: the **national tax code** of the service (six digits: item, sub-item and detail of the national
list), the municipality's **own code** (up to 20 characters, for a gateway whose layout is municipal), how the ISSQN is charged
(`taxable`, `immune`, `export` or `not_incident`, the four values of the national standard's `tribISSQN` that a service of the company's own can
have) and, when the company sets it, the **rate** (0 to 100, two decimals, an exact decimal). A product nature carries none, and none is
required to save the settings: a nature without them is a service the company has not yet described, and an NFS-e under it is refused.

**The product decides none of it.** It has no table of the national list, does not know which services a municipality taxes, and does not
choose between `taxable` and `immune`: they are the company's and its accountant's (verify with an accountant), as the CFOP and the CST of
an NF-e are (ADR 0074). Only the shape is checked, in the domain and again in a `CHECK`.

### Storage and the trail

- `fiscal_settings.city_ibge_code`, and `fiscal_natures.national_tax_code`, `municipal_service_code`, `iss_taxation`, `iss_rate`, with `CHECK`s
  that make the national code and the taxation come together, belong only to a service, and keep their shapes (the test of a `NULL` is explicit,
  since a `CHECK` passes on `NULL`). It is a new migration; none merged is edited.
- The trail's `natures` text keeps its seven parts for what it already recorded and **adds an eighth only when a service has terms**
  (`national code:municipal code:taxation:rate`), so no past entry changes meaning and a change of the rate shows. `city_ibge_code` is a field
  of its own in the trail.

## Consequences

- Settings saved before this keep working: the new fields are empty, and nothing but an NFS-e asks for them.
- The next slice can build the NFS-e from the settings alone, without calling a registry for the issuer.
- A company that issues services under several natures says the terms once per nature, which is what an accountant already thinks in.

## What this does not decide, and is left to 5b

The numbering of the DPS (the national standard has the **issuer** number it, unlike the NF-e, where the gateway does), how each gateway names an
NFS-e (Focus NFe by the reference, NFE.io by its own id) and what each cancels and corrects. They are decided with the adapters, which are written
from the gateways' published documentation and **cannot be exercised without a real certificate, a gateway account and a municipality that has
joined the national standard**.
