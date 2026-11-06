import type { SaleStatus } from "../api";

/** How a sale's status reads to a person; the keys are the API's and never change. */
export const SALE_STATUS_LABELS: Record<SaleStatus, string> = {
  open: "Pedido",
  completed: "Venda concluída",
  cancelled: "Cancelada",
};
