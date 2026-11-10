import { createHmac, timingSafeEqual } from "node:crypto";

/** How far from now a delivery's timestamp may be, in seconds: a replay of an old one is refused. */
export const TOLERANCE_SECONDS = 300;

/** The signature of a delivery: `v1,` and the base64 HMAC-SHA256 of `{id}.{timestamp}.{body}`, the key being the secret's own text. */
export function sign(secret, id, timestamp, body) {
  return `v1,${createHmac("sha256", secret).update(`${id}.${timestamp}.${body}`).digest("base64")}`;
}

/**
 * Whether a delivery of conta-verde is genuine and fresh. The header may hold several signatures separated by a space while a
 * replaced secret still signs; the comparison is constant time and made on the raw body, before anything is parsed.
 */
export function verify({ secret, id, timestamp, signatures, body, nowSeconds }) {
  if (!id || !timestamp || !signatures) return false;
  const sent = Number(timestamp);
  if (!Number.isInteger(sent) || Math.abs(nowSeconds - sent) > TOLERANCE_SECONDS) return false;
  const expected = Buffer.from(sign(secret, id, timestamp, body));
  return signatures
    .split(" ")
    .some((candidate) => {
      const given = Buffer.from(candidate);
      return given.length === expected.length && timingSafeEqual(given, expected);
    });
}
