import type { ApiKey } from "../api";
import { msg, type Translate } from "../i18n";

/** How an area reads on screen, as `msg` keys: the screen that shows one translates it. The keys are the API's and never shown. */
export const AREA_LABELS: Record<string, string> = {
  parties: msg("Customers and suppliers"),
  products: msg("Products and services"),
  finance: msg("Financial"),
  sales: msg("Sales"),
  purchases: msg("Purchases and stock"),
};

export type Reach = "none" | "read" | "write";

export const REACH_LABELS: Record<Reach, string> = {
  none: msg("No access"),
  read: msg("View only"),
  write: msg("View and change"),
};

/** `sales:write` as "Vendas: consulta e altera". */
export function scopeLabel(scope: string, t: Translate): string {
  const [area, access] = scope.split(":");
  return `${t(AREA_LABELS[area] ?? area)}: ${t(REACH_LABELS[access === "write" ? "write" : "read"]).toLowerCase()}`;
}

/** What a key reaches, one scope text per area that is not "none". */
export function scopesOf(reach: Record<string, Reach>): string[] {
  return Object.entries(reach)
    .filter(([, level]) => level !== "none")
    .map(([area, level]) => `${area}:${level}`);
}

// Portuguese says a key is "Ativa", not "Ativo", and English has one word: the `{feminine}` part is empty and only tells the two apart.
const STATUS_TEXTS: Record<ApiKey["status"], string> = {
  active: msg("Active{feminine}"),
  expired: msg("Expired{feminine}"),
  revoked: msg("Revoked{feminine}"),
};

export const statusLabel = (status: ApiKey["status"], t: Translate) => t(STATUS_TEXTS[status], { feminine: "" });

/** How long a new key lives, in hours; `undefined` is "never". */
export const EXPIRY_OPTIONS: { label: string; hours: number | undefined }[] = [
  { label: msg("Does not expire"), hours: undefined },
  { label: msg("30 days"), hours: 30 * 24 },
  { label: msg("90 days"), hours: 90 * 24 },
  { label: msg("1 year"), hours: 365 * 24 },
];

/** How long the old key goes on working after a rotation, in hours. */
export const GRACE_OPTIONS: { label: string; hours: number }[] = [
  { label: msg("Stop now"), hours: 0 },
  { label: msg("1 hour"), hours: 1 },
  { label: msg("1 day"), hours: 24 },
  { label: msg("7 days"), hours: 168 },
];
