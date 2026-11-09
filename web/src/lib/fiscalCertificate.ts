/** How many days before the certificate expires the page starts warning: a month is time to renew it. */
export const CERTIFICATE_WARNING_DAYS = 30;

export type CertificateStatus =
  | { kind: "unknown" }
  | { kind: "valid"; daysLeft: number }
  | { kind: "expiring"; daysLeft: number }
  | { kind: "expired"; daysAgo: number };

const DAY_MS = 24 * 60 * 60 * 1000;

function daysFromEpoch(iso: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (match === null) return null;
  return Math.round(Date.UTC(Number(match[1]), Number(match[2]) - 1, Number(match[3])) / DAY_MS);
}

/**
 * Where the certificate stands on `today`, from the day it expires as the person typed it (the certificate is at the
 * gateway, so this is all the product knows). The certificate is good through that day.
 */
export function certificateStatus(validUntil: string, today: string): CertificateStatus {
  const until = daysFromEpoch(validUntil);
  const now = daysFromEpoch(today);
  if (until === null || now === null) return { kind: "unknown" };
  const daysLeft = until - now;
  if (daysLeft < 0) return { kind: "expired", daysAgo: -daysLeft };
  return daysLeft <= CERTIFICATE_WARNING_DAYS ? { kind: "expiring", daysLeft } : { kind: "valid", daysLeft };
}
