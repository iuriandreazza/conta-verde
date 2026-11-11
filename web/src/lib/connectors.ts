import type { Connector } from "../api";
import { msg, type Translate } from "../i18n";

/** What each kind of the catalogue does and needs, in words, as `msg` keys. The keys are the API's and never shown. */
export const KIND_DESCRIPTIONS: Record<string, { what: string; needs: string }> = {
  woocommerce: {
    what: msg("Brings the paid orders of a WooCommerce store in as sales orders and tells the store when the sale is completed or cancelled here."),
    needs: msg("A server where the package runs, with a public https address, and the store's REST API key (which stays only in the package, never here)."),
  },
};

export const STATE_LABELS: Record<Connector["state"], string> = {
  off: msg("Turned off"),
  on: msg("Turned on"),
  attention: msg("Needs attention"),
};

export const STATE_BADGES: Record<Connector["state"], string> = {
  off: "badge badge-muted",
  on: "badge badge-dot badge-ok",
  attention: "badge badge-dot badge-warning",
};

/** Why a connector that is on needs the person, read from what the server says it still has. */
export function attentionReason(connector: Connector, t: Translate): string | null {
  if (connector.state !== "attention") return null;
  if (!connector.key_usable && !connector.subscription_on) return t("The key was revoked and the webhook was turned off on another screen. Turn the connector off and on again.");
  if (!connector.key_usable) return t("The key was revoked or expired in API keys. Turn the connector off and on again to get another.");
  return t("The webhook was turned off or deleted in Webhooks. Turn the connector off and on again.");
}

/** Where the package and its instructions are, in the source the installation runs. */
export const packageUrl = (sourceUrl: string, path: string) => `${sourceUrl.replace(/\/$/, "")}/tree/main/${path}`;
