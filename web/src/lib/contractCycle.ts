import type { BillingCycle, ContractStatus } from "../api";

/** How a contract's cycle and status read to a person; the keys are the API's and never change. */
export const CYCLE_LABELS: Record<BillingCycle, string> = {
  monthly: "Todo mês",
  bimonthly: "A cada 2 meses",
  quarterly: "A cada 3 meses",
  semiannual: "A cada 6 meses",
  yearly: "Todo ano",
};

export const CONTRACT_STATUS_LABELS: Record<ContractStatus, string> = {
  active: "Ativo",
  paused: "Pausado",
  terminated: "Encerrado",
};
