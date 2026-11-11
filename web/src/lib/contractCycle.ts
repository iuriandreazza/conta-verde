import type { BillingCycle, ContractStatus } from "../api";
import { msg } from "../i18n";

/** How a contract's cycle and status read to a person (the place that shows them translates them); the keys are the API's and never change. */
export const CYCLE_LABELS: Record<BillingCycle, string> = {
  monthly: msg("Every month"),
  bimonthly: msg("Every 2 months"),
  quarterly: msg("Every 3 months"),
  semiannual: msg("Every 6 months"),
  yearly: msg("Every year"),
};

export const CONTRACT_STATUS_LABELS: Record<ContractStatus, string> = {
  active: msg("Active"),
  paused: msg("Paused"),
  terminated: msg("Terminated"),
};
