---
title: "ADR 0091: The PWA: installable, with offline reading under written cache rules"
status: accepted
date: 2026-11-10
tags: [adr, web, pwa, offline, security, privacy, phase-8]
---

# ADR 0091: The PWA: installable, with offline reading under written cache rules

## Context

[ADR 0086](0086-integrations-and-mobile-plan.md) planned the last slice of Phase 8: the web app installable on a phone, opening
without a network and showing recent data. A cache is another place a company's data lives, on a device that may be shared or lost,
so the plan wrote the cache rules down before any code. This record says what was built, what **changed from ADR 0086**, and what the
tests and a real browser prove.

## Decision

### Installable

`site.webmanifest` (name, `id`, `scope`, `start_url`, `display: standalone`, the theme colors and the icons rendered from the logo)
and a **service worker** at `/sw.js`. The worker is its own entry of the build (`web/src/pwa/sw.ts`, built by a Vite plugin into the
same folder under a fixed name, because a worker's address is its identity) and is **registered only in a production build**:
development serves none, and a stale worker would hide every change. The reverse proxy serves `/sw.js` and `/index.html` with
`Cache-Control: no-cache` (`deploy/nginx.conf`), so a new build's rules reach the device.

### Offline reading is opt-in, per device

The page **Uso offline** (`/offline`, linked from the top bar) turns it on **on this device** and says before the person presses what
it leaves there: a copy of what they consult, **including personal data of customers** (document, address, contacts), for at most **7
days** or until they **sign out**, whichever comes first; use it only on a device that is theirs; offline they only **read**; and
what is never kept. The choice is **the person's, not the browser's** (kept per user id): someone else who signs in on the same device starts with it off and the first person's copy is emptied. Turning it off deletes the copy at once. Off (the default), the worker keeps **no data at all**; the
application's own files are still kept, so the app opens offline either way (and then asks for what it cannot read: nothing).
**No offline writes**: a form that cannot reach the server says so (`Sem conexão com o servidor… não dá para salvar offline`, instead
of the generic failure), and a banner says, on every screen, that the connection is gone or that what is shown is the device's copy
**from a given time** (rule 7).

### The cache rules (`web/src/pwa/offline.ts`, the one module the worker, the page and the tests all run)

1. **An allowlist of what may be cached, not a list of what may not** (ordinary JSON carries no `Cache-Control`, so a `no-store` rule
   would hardly ever apply). Only `GET`, same origin, of `/api/companies/{id}/…` for: `parties`, `products`, `entries`, `sales`,
   `quotes`, `balances`, `cash-flow`, `stock/positions` (and `/summary`), `reports/overdue`, `sales-reports`, `statement-lines` (the
   dashboard's sources and the lists a person opens), with their query strings; and `GET /api/auth/me`. **Everything else goes to the
   network untouched**, which covers by construction the things ADR 0086 listed as never kept: `members`, `api-keys`,
   `webhook-subscriptions`, `connectors`, `gateway-settings`, `accounting`, `fiscal`, attachments, exports, histories, payment links,
   `/public`, `/v1`, every other `/auth` call, and every method but `GET`. A test lists these paths one by one.
2. **Changed from ADR 0086: `GET /auth/me` is kept** (the profile: name, e-mail, companies and what the person may do), under the
   person's own name and only while the mode is on, because **the application cannot start offline without it** (a reload offline
   would show the sign-in page). It follows every rule below, and any `401` of it clears everything.
3. **Per person and per company.** The worker cannot read the HttpOnly session cookie, so **the page tells it who is signed in**: it
   writes the user id and whether the mode is on to **IndexedDB** (a worker is stopped and started at will) and the worker reads it
   there. **Changed from ADR 0086** (which had the page *message* the worker, the worker writing it): the page writes the record itself,
   which is the same thing without a message that can be lost while the worker sleeps. Every data cache is named
   `data:{user}:{company}` (`…:session` for the profile); **a different person empties the others' caches before anything is read**,
   and nothing is read or written while nobody is known or the mode is off.
4. **Cleared at sign-out and on any `401`, clearing first and calling the network second** (so signing out offline still clears):
   the page deletes the data caches and the identity record **itself** (Cache Storage and IndexedDB are the page's to reach, so it
   never depends on the worker being awake; each part is tried even if the other fails) **before** it calls `/auth/logout`, a test
   checks the order, and if the logout then fails the page tells the worker who is signed in again; the worker does the same
   clearing when a data request answers `401`. **A request that was away when the clearing came does not undo it**: before it writes,
   the worker reads the identity again and writes nothing for someone who signed out, turned the mode off or was replaced by another
   person meanwhile (the profile is also kept only if it is the profile of the person the page named). An entry **expires after 7
   days**: a copy older than that is never served, **and a sweep deletes expired entries nobody asks for again** (when the page
   loads, and once an hour in the worker when it keeps something new); an entry stamped in the future is treated as expired too.
5. **Bounded and harmless:** a body over **1 MB** is not kept, only **JSON** with status `200` is, and at most **40 answers per person
   and company** (the oldest goes first; the search terms in an address are therefore on the device too, and the page says so). The
   network is asked **first**: when it answers, that answer is the one; only a failed request, or a server that is there but cannot
   answer (**502, 503, 504**), falls back to the copy, marked `x-offline-from-cache` with the time it was stored
   (`x-offline-cached-at`). A `500` of the application is its answer, never replaced. **Whatever goes wrong with the storage (a full
   or refused IndexedDB or Cache Storage), the network's answer still reaches the page**: the offline mode is a convenience and never
   what stands between a person and the server.
6. **The application's own files** are not data: only files under `/assets/` (named by their content, at most 120 kept) are cached
   cache-first, and a missing file that the server answers with the page is not kept; the icons and the manifest, which have no
   hash, are the network's. A page navigation is answered by the network, falling back to the last `index.html` (the shell, no data);
   the worker's own address is never answered with it. A new worker deletes the caches of any other version.
7. **What the screen says.** A banner says the connection is gone, and that what is shown is the device's copy **from a given time**,
   **keeping that label when the network comes back until what was shown from the copy is read live again** (by address: one card read
   live does not hide another that is still the copy) with a button to refresh. Offline, a form that cannot reach the server says so
   (a `TypeError` of `fetch`; a `TypeError` that is a bug keeps the generic message).

**Known limit:** while offline the application trusts the profile it kept: a session that the server ended meanwhile is only found
out when the network is back (then any `401` clears the device). That is the 7-day window above, which the page says.

## What the tests prove

- `web/src/pwa/offline.test.ts` (against a **fake `caches`, `fetch` and identity store**): the allowlist path by path (what is let in
  and the long list of what never is, every method, another origin, a malformed company id); a good answer kept under the person and
  company and served when the network is gone with its time; network first; a `500` never kept; two companies of one person apart;
  **another person's caches emptied before anything is read**; nothing at all while off or unknown; `401` clearing first and forgetting
  the person; seven days to the millisecond; the megabyte and the 40-entry bounds; non-JSON not kept; assets and the shell.
- `client.test.ts` and `offline.test.tsx`: telling the worker who is signed in, turning on and off (off clears at once), sign-out
  clearing **before the server is asked**, a device that refuses storage, the banner and its label lifting on a live answer, the page's
  text about what stays on the device, and the plain message for a lost network.
- **`scripts/e2e-pwa.py` in a real Chromium** (Playwright) against a production build and the real API: the worker registers and takes
  control, the list is cached under this person and company (and `members` is not), **offline the app opens and shows the cached list
  with the banner**, an un-allowlisted route fails as offline, back online the label goes, and signing out leaves no data cache. It is a
  script run by hand, not a CI job (it needs a browser and a running API).

## Not validated

- Only **Chromium** was tried. Safari and Firefox implement service workers, IndexedDB and Cache Storage differently at the edges (for
  instance how long iOS keeps a PWA's storage, and its own limits); the rules are the same code but nothing was run there.
- The Claude desktop app's built-in browser pane **cannot register a service worker at all**, so nothing was verified there.
- The install prompt and the home screen icon depend on the browser's own criteria (manifest, worker, HTTPS): not exercised on a phone.

## Consequences

- An installation behind a reverse proxy must serve `/sw.js` and `/index.html` with `Cache-Control: no-cache` (`deploy/nginx.conf`
  does), and over HTTPS, where a browser allows a worker (localhost is exempt).
- Adding something to the allowlist is one line in `offline.ts` and a test, and a sentence on the page; it changes what a device keeps,
  so it is a decision, not a default.
- The cache rules live in one module with no globals, so a change to them is tested with fakes and cannot drift between the worker and
  the page.

## Revisit when

- A phone shows storage evicted early or a different behavior on iOS: say it here and shorten the window or the entries.
- The profile should not be kept: the app would need an offline shell that does not need to know who is signed in.
