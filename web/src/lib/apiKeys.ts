import type { ApiKey } from "../api";

/** How an area reads on screen. The keys are the API's and never shown. */
export const AREA_LABELS: Record<string, string> = {
  parties: "Clientes e fornecedores",
  products: "Produtos e serviços",
  finance: "Financeiro",
  sales: "Vendas",
  purchases: "Compras e estoque",
};

export type Reach = "none" | "read" | "write";

export const REACH_LABELS: Record<Reach, string> = {
  none: "Sem acesso",
  read: "Só consulta",
  write: "Consulta e altera",
};

/** `sales:write` as "Vendas: consulta e altera". */
export function scopeLabel(scope: string): string {
  const [area, access] = scope.split(":");
  return `${AREA_LABELS[area] ?? area}: ${REACH_LABELS[access === "write" ? "write" : "read"].toLowerCase()}`;
}

/** What a key reaches, one scope text per area that is not "none". */
export function scopesOf(reach: Record<string, Reach>): string[] {
  return Object.entries(reach)
    .filter(([, level]) => level !== "none")
    .map(([area, level]) => `${area}:${level}`);
}

export const STATUS_LABELS: Record<ApiKey["status"], string> = {
  active: "Ativa",
  expired: "Expirada",
  revoked: "Revogada",
};

/** How long a new key lives, in hours; `undefined` is "never". */
export const EXPIRY_OPTIONS: { label: string; hours: number | undefined }[] = [
  { label: "Não expira", hours: undefined },
  { label: "30 dias", hours: 30 * 24 },
  { label: "90 dias", hours: 90 * 24 },
  { label: "1 ano", hours: 365 * 24 },
];

/** How long the old key goes on working after a rotation, in hours. */
export const GRACE_OPTIONS: { label: string; hours: number }[] = [
  { label: "Parar agora", hours: 0 },
  { label: "1 hora", hours: 1 },
  { label: "1 dia", hours: 24 },
  { label: "7 dias", hours: 168 },
];
