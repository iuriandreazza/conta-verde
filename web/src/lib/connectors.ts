import type { Connector } from "../api";

/** What each kind of the catalogue does and needs, in words. The keys are the API's and never shown. */
export const KIND_DESCRIPTIONS: Record<string, { what: string; needs: string }> = {
  woocommerce: {
    what: "Traz os pedidos pagos de uma loja WooCommerce como pedidos de venda e avisa a loja quando a venda é concluída ou cancelada aqui.",
    needs: "Um servidor onde o pacote rode, com endereço https público, e a chave de API REST da loja (que fica só no pacote, nunca aqui).",
  },
};

export const STATE_LABELS: Record<Connector["state"], string> = {
  off: "Desligado",
  on: "Ligado",
  attention: "Precisa de atenção",
};

export const STATE_BADGES: Record<Connector["state"], string> = {
  off: "badge badge-muted",
  on: "badge badge-dot badge-ok",
  attention: "badge badge-dot badge-warning",
};

/** Why a connector that is on needs the person, read from what the server says it still has. */
export function attentionReason(connector: Connector): string | null {
  if (connector.state !== "attention") return null;
  if (!connector.key_usable && !connector.subscription_on) return "A chave foi revogada e o webhook foi desligado em outra tela. Desligue e ligue o conector de novo.";
  if (!connector.key_usable) return "A chave foi revogada ou expirou em Chaves de API. Desligue e ligue o conector de novo para ter outra.";
  return "O webhook foi desligado ou apagado em Webhooks. Desligue e ligue o conector de novo.";
}

/** Where the package and its instructions are, in the source the installation runs. */
export const packageUrl = (sourceUrl: string, path: string) => `${sourceUrl.replace(/\/$/, "")}/tree/main/${path}`;
