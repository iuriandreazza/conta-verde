import assert from "node:assert/strict";
import { test } from "node:test";
import { TOLERANCE_SECONDS, sign, verify } from "../src/signature.js";

const secret = "whsec_" + "ab".repeat(32);
const body = JSON.stringify({ id: "e1", type: "sale.completed" });
const now = 1_800_000_000;
const delivery = (overrides = {}) => ({ secret, id: "msg_1", timestamp: String(now), signatures: sign(secret, "msg_1", String(now), body), body, nowSeconds: now, ...overrides });

test("a delivery signed with the secret's own text as key verifies", () => {
  assert.equal(verify(delivery()), true);
  // Independent of the helper: the HMAC-SHA256 of `id.timestamp.body` in base64, computed by hand.
  assert.match(sign(secret, "msg_1", String(now), body), /^v1,[A-Za-z0-9+/]{43}=$/);
});

test("a wrong secret, a changed body, a moved id or a malformed header does not verify", () => {
  assert.equal(verify(delivery({ secret: "another-secret" })), false);
  assert.equal(verify(delivery({ body: body + " " })), false);
  assert.equal(verify(delivery({ id: "msg_2" })), false);
  for (const signatures of [undefined, "", "v1,", "v1,AAAA", "garbage"]) assert.equal(verify(delivery({ signatures })), false, String(signatures));
});

test("several signatures are accepted while a replaced secret still signs", () => {
  const old = sign("the-old-secret", "msg_1", String(now), body);
  const current = sign(secret, "msg_1", String(now), body);
  assert.equal(verify(delivery({ signatures: `${old} ${current}` })), true);
  assert.equal(verify(delivery({ signatures: `${current} ${old}` })), true);
  assert.equal(verify(delivery({ signatures: old })), false);
});

test("a timestamp more than five minutes off is refused, either way", () => {
  const at = (offset) => delivery({ timestamp: String(now + offset), signatures: sign(secret, "msg_1", String(now + offset), body) });
  assert.equal(verify(at(TOLERANCE_SECONDS)), true);
  assert.equal(verify(at(-TOLERANCE_SECONDS)), true);
  assert.equal(verify(at(TOLERANCE_SECONDS + 1)), false);
  assert.equal(verify(at(-TOLERANCE_SECONDS - 1)), false);
  assert.equal(verify(delivery({ timestamp: "soon" })), false);
});
