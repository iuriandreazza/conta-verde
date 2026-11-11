import type { WebhookDelivery } from "../api";
import { msg, type Translate } from "../i18n";

/** How an event reads on screen, as `msg` keys: the screen that shows one translates it. The keys are the API's and never shown. */
export const EVENT_LABELS: Record<string, string> = {
  "sale.created": msg("Order created"),
  "sale.completed": msg("Sale completed"),
  "sale.cancelled": msg("Sale cancelled"),
  "charge.paid": msg("Charge paid"),
  "stock.below_minimum": msg("Stock below the minimum"),
  "webhook.test": msg("Test"),
};

export const eventLabel = (kind: string, t: Translate) => t(EVENT_LABELS[kind] ?? kind);

export const DELIVERY_LABELS: Record<WebhookDelivery["status"], string> = {
  pending: msg("Waiting"),
  delivered: msg("Delivered"),
  failed: msg("Gave up"),
};

/** What the subscription listens to, in words: no events asked for is every one. */
export function eventsText(events: string[], t: Translate): string {
  return events.length === 0 ? t("All events") : events.map((kind) => eventLabel(kind, t)).join(", ");
}
