import type { QuoteStatus } from "../api";
import { msg } from "../i18n";

/** How a quote's status reads to a person (the place that shows it translates it); the keys are the API's and never change. */
export const QUOTE_STATUS_LABELS: Record<QuoteStatus, string> = {
  draft: msg("Draft"),
  sent: msg("Sent"),
  approved: msg("Approved"),
  declined: msg("Declined"),
  converted: msg("Became a sale"),
};
