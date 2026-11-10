import type { WebhookDelivery } from "../api";

/** How an event reads on screen. The keys are the API's and never shown. */
export const EVENT_LABELS: Record<string, string> = {
  "sale.created": "Pedido criado",
  "sale.completed": "Venda concluída",
  "sale.cancelled": "Venda cancelada",
  "charge.paid": "Cobrança paga",
  "stock.below_minimum": "Estoque abaixo do mínimo",
  "webhook.test": "Teste",
};

export const eventLabel = (kind: string) => EVENT_LABELS[kind] ?? kind;

export const DELIVERY_LABELS: Record<WebhookDelivery["status"], string> = {
  pending: "Aguardando",
  delivered: "Entregue",
  failed: "Desistiu",
};

/** What the subscription listens to, in words: no events asked for is every one. */
export function eventsText(events: string[]): string {
  return events.length === 0 ? "Todos os eventos" : events.map(eventLabel).join(", ");
}
