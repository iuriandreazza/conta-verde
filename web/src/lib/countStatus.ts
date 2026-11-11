import type { InventoryCountStatus } from "../api";
import { msg } from "../i18n";

/** How an inventory count's status reads to a person; the keys are the API's and never change. */
export const COUNT_STATUS_LABELS: Record<InventoryCountStatus, string> = {
  open: msg("Count open"),
  applied: msg("Applied"),
  cancelled: msg("Count cancelled"),
};

/** The badge of a status: the dot and the word say it, the color only helps. */
export function countStatusClass(status: InventoryCountStatus): string {
  switch (status) {
    case "applied":
      return "badge badge-dot badge-ok";
    case "cancelled":
      return "badge badge-dot badge-critical";
    default:
      return "badge badge-dot badge-warning";
  }
}
