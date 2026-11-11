import type { PurchaseStatus } from "../api";
import { msg } from "../i18n";

/** How a purchase order's status reads to a person; the keys are the API's and never change. */
export const PURCHASE_STATUS_LABELS: Record<PurchaseStatus, string> = {
  draft: msg("Draft"),
  approved: msg("Approved"),
  partially_received: msg("Partially received"),
  received: msg("Received"),
  cancelled: msg("Purchase cancelled"),
};

/** The badge of a status: the dot and the word say it, the color only helps. */
export function purchaseStatusClass(status: PurchaseStatus): string {
  switch (status) {
    case "received":
      return "badge badge-dot badge-ok";
    case "cancelled":
      return "badge badge-dot badge-critical";
    default:
      return "badge badge-dot badge-warning";
  }
}
