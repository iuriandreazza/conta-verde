---
title: "ADR 0077: Reading a company's books from the cloud ERP's API"
status: proposed
date: 2026-11-10
tags: [adr, import, migration]
---

# ADR 0077: Reading a company's books from the cloud ERP's API

## Context

[ADR 0064](0064-entry-import-from-a-file.md) imports entries from a CSV that a script converts from the spreadsheets the
old system exports. Spreadsheets lose information (one row per category and cost center piece, no stable ids) and an
import made from them had to be redone. The old system has a public REST API (OAuth 2.0, `api-v2.contaazul.com`) that
returns the same registers with their own ids.

## Decision

A reader outside the application, `scripts/erp-api-reader.py` (standard library only, like the converter it feeds):

- `login` runs the authorization code flow against `api-v2.contaazul.com` with a loopback redirect and keeps the tokens in
  `~/.config/conta-verde/erp-api-token.json` (mode 0600, outside the repository). The client id and secret come from
  the environment, never from an argument or a file in the repository.
- `fetch` renews the token when it must and writes one JSON file per register, as the API answered: categories (income
  and expense), cost centers, financial accounts, people, products, receivables and payables. The two financial
  listings need a due-date window, read in windows of 90 days. A 401 renews the token once; a 429 or 5xx waits and tries
  again; any other answer stops with its text.
- The files are the input of a converter that writes the files the existing importers take (parties and products,
  [ADR 0025](0025-csv-import.md); entries, ADR 0064). Nothing here writes to conta-verde, and the converter's output is
  checked before it is imported.

The endpoints and parameters are those of the vendor's public client; **the shape of each answer is confirmed against a
real account before the converter is written**, which is why the reader keeps the raw files.

## Consequences

- No Rust, no migration, no new dependency; the architecture layers are untouched.
- Tests use a local stand-in for the API, never the real one.
- The vendor's name appears only in the API addresses the tool must call, never in the UI or the docs.

## Revisit when

- Imports from this source are routine, which would justify a connector inside the application (a port with a job,
  [ADR 0059](0059-job-queue.md)) instead of a script.
