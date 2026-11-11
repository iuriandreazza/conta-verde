import { useCallback } from "react";
import { api } from "../api";
import { msg, useT, type Translate } from "../i18n";
import { formatDecimal } from "../lib/decimal";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  kind: msg("Type"),
  name: msg("Name"),
  sku: msg("SKU"),
  unit: msg("Unit"),
  ncm: msg("NCM"),
  sale_price: msg("Sale price"),
  cost_price: msg("Cost price"),
  minimum_stock: msg("Minimum stock"),
  active: msg("Active"),
};

const KIND_LABELS: Record<string, string> = { product: msg("Product"), service: msg("Service") };

function label(field: string, t: Translate): string {
  const text = FIELD_LABELS[field];
  return text === undefined ? field : t(text);
}

function display(field: string, value: string | null, t: Translate): string {
  if (value === null) return "—";
  if (field === "active") return value === "true" ? t("yes") : t("no");
  if (field === "kind") return value in KIND_LABELS ? t(KIND_LABELS[value]) : value;
  if (field === "sale_price" || field === "cost_price") return `R$ ${formatDecimal(value, 2)}`;
  if (field === "minimum_stock") return formatDecimal(value);
  return value;
}

export function ProductHistory({ companyId, productId }: { companyId: string; productId: string }) {
  const t = useT();
  const load = useCallback(() => api.productHistory(companyId, productId), [companyId, productId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => label(field, t)}
      valueLabel={(field, value) => display(field, value, t)}
      emptyMessage={t("No records: this record predates the history. The next changes will appear here.")}
    />
  );
}
