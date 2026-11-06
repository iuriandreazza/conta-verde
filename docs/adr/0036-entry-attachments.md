---
title: "ADR 0036: Files attached to entries, and where their bytes are kept"
status: accepted
date: 2026-11-06
tags: [adr, finance, attachments, storage, phase-2]
---

# ADR 0036: Files attached to entries, and where their bytes are kept

## Context

An entry ([ADR 0029](0029-financial-entries.md)) is usually backed by a paper: the invoice, the receipt, the
contract. The roadmap lists attachments with the entries, and [ADR 0009](0009-postgres-only-background-jobs.md)
already decided that their bytes go to the local filesystem or an S3-compatible bucket behind a `BlobStore`
port, "the only optional extra" next to PostgreSQL. Nothing of it was built.

## Options considered

- **Bytes in PostgreSQL** (`bytea`). One thing to back up and one transaction for record and bytes. But every
  file passes through the database's memory and WAL, backups grow with the files, and it contradicts ADR 0009.
- **Bytes on the filesystem or S3 behind a port, the record in PostgreSQL.** Two places to keep consistent,
  but the database stays small, an adapter for S3 is one more file, and it is what ADR 0009 said.
- **Multipart upload.** The browser's natural form, but it needs a parser dependency and its own limits. The
  alternative is the file as the **body of the request**, with its type in `Content-Type` and its name in the
  query: no dependency, and `fetch(url, { body: file })` sends it.

## Decision

### The record and the bytes

An **attachment** (`domain::Attachment`) belongs to one entry of the company: a file name, a content type and
a size in bytes. Its bytes live in a `BlobStore` (`application`) under the **company and the attachment id**,
both ids the program made, never anything a person typed, so a path cannot be steered out of a directory.
The port is three methods (`put`, `get`, `delete`) and knows nothing of entries or audit. Adapters:
`LocalBlobStore` (a directory, `ATTACHMENTS_DIR`, default `./data/attachments`; written aside and renamed so a
reader never sees half a file) and `InMemoryBlobStore` for tests. S3 is one more adapter, not built.

The record is in `entry_attachments` with a composite foreign key `(company_id, entry_id)`, a `CHECK` for every
rule one row shows (name, type list, size) and a `seq` that keeps the order they were attached in.

### Never edited, removed by marking

Like a settlement or a transfer, an attachment is never edited or deleted: **removing** it sets `removed`,
drops the bytes and keeps the record, so the trail says who attached what and who took it away. Each write
carries its `AuditEntry` in the same transaction ([ADR 0008](0008-audit-trail.md); `AuditEntity::Attachment`).
A removed attachment is not listed and cannot be downloaded.

### Ordering the two stores

The **bytes go first**, then the record with its audit entry; if the record write fails the bytes are taken
back, so a record never points at a file that is not there. On removal the **record goes first**, then the
bytes; bytes that could not be dropped are unreachable clutter and do not fail a removal that has happened. A
crash between the steps can leave an orphan file (or a `.partial` one, when a write is cut short), never a
dangling record. Nothing sweeps orphans yet. A download that races a removal between reading the record and
reading the bytes answers `500` instead of `404`; the window is a few milliseconds and the next read is right.

### What is taken

- **Size:** from 1 byte to 10 MB (`MAX_ATTACHMENT_BYTES`), read by hand with a limit and a `Content-Length`
  check in front, answered `413` in the API's JSON shape.
- **Count:** 20 per entry at once, the removed ones not counting (`409`). The use case counts and then writes,
  so two uploads at the same instant can pass 20 by one; a limit this size is a guard against clutter, not a
  quota, and the database does not hold it.
- **Type:** a short list: PDF, PNG, JPEG, WebP, plain text, CSV and XML (`422` otherwise). Nothing a browser
  would run when opened (HTML, SVG, scripts) is on it. The type is what the browser said and is trusted no
  further than naming what is served back.
- **Name:** one line, at most 200 characters, no slash, backslash or control character, not `.` or `..`.

### Serving a file

`GET .../attachments/{id}` answers the bytes with the stored type, `Content-Disposition: attachment` (the
name twice, a plain one and an RFC 6266 `filename*`), `X-Content-Type-Options: nosniff` and
`Cache-Control: private, no-store`: always a download, never a page on the app's own origin. The session
cookie travels with a plain link, so the web app needs no fetch-and-blob step.

### Routes and roles

Under the entry, so the existing role area applies with no new segment: `GET` and `POST`
`/companies/{id}/entries/{entry_id}/attachments` (list, attach with `?name=`), `GET` and `DELETE`
`/companies/{id}/entries/{entry_id}/attachments/{attachment_id}`. All are `finance`: reading and downloading
need `read`, attaching and removing need `write`. An entry or attachment of another company answers `404`.

### Web

An **Anexos** action on each entry row opens a panel: the files with their size and a download link, a button
to attach (the picker offers the listed extensions; an empty file or one past the limit is refused before it
is sent), and a remove button, both only for roles that can write.

## Consequences

- Backups now have two parts: the database and `ATTACHMENTS_DIR`. The directory holds the only copy of what
  people attach.
- A file is held in memory while it is read and written (at most 10 MB per request). Streaming is not worth
  its complexity at this size.
- Attaching is a write to an entry that does not touch the entry: a settled entry can still get its receipt.
- No virus scanning, no thumbnails, no versions of a file: attach again and remove the old one.

## Revisit when

- An S3 adapter is wanted (a hosted installation with several instances sharing no disk).
- Orphan files from crashes pile up: add a sweep that lists the store against the records.
- Files larger than 10 MB, or many more than 20 per entry, are a real need: stream and count in the database.
- Other documents (parties, quotes) take attachments: lift the record out of the entry's table.
