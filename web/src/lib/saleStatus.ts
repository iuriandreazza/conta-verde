import type { SaleStatus } from "../api";
import { msg } from "../i18n";

/** How a sale's status reads to a person (the place that shows it translates it); the keys are the API's and never change. */
export const SALE_STATUS_LABELS: Record<SaleStatus, string> = {
  open: msg("Order"),
  completed: msg("Completed sale"),
  cancelled: msg("Cancelled"),
};
