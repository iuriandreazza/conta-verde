import { useCallback } from "react";
import { api } from "../api";
import { formatDecimal } from "../lib/decimal";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  kind: "Tipo",
  name: "Nome",
  sku: "SKU",
  unit: "Unidade",
  ncm: "NCM",
  sale_price: "Preço de venda",
  cost_price: "Preço de custo",
  minimum_stock: "Estoque mínimo",
  active: "Ativo",
};

const KIND_LABELS: Record<string, string> = { product: "Produto", service: "Serviço" };

function label(field: string): string {
  return FIELD_LABELS[field] ?? field;
}

function display(field: string, value: string | null): string {
  if (value === null) return "—";
  if (field === "active") return value === "true" ? "sim" : "não";
  if (field === "kind") return KIND_LABELS[value] ?? value;
  if (field === "sale_price" || field === "cost_price") return `R$ ${formatDecimal(value, 2)}`;
  if (field === "minimum_stock") return formatDecimal(value);
  return value;
}

export function ProductHistory({ companyId, productId }: { companyId: string; productId: string }) {
  const load = useCallback(() => api.productHistory(companyId, productId), [companyId, productId]);
  return (
    <History
      load={load}
      fieldLabel={label}
      valueLabel={display}
      emptyMessage="Sem registros: este cadastro é anterior ao histórico. As próximas alterações aparecerão aqui."
    />
  );
}
