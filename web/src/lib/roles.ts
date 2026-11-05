import type { Role } from "../api";

/** How a role reads on screen. The keys are the API's and never shown. */
export const ROLE_LABELS: Record<Role, string> = {
  owner: "Dono",
  admin: "Administrador",
  financial: "Financeiro",
  sales: "Vendas",
  stock: "Estoque",
  accountant: "Contador",
  viewer: "Somente leitura",
};

/** What a role can do, in a line, for the person choosing one to invite. */
export const ROLE_HINTS: Record<Role, string> = {
  owner: "Tudo, e é quem criou a empresa.",
  admin: "Tudo, incluindo convidar pessoas.",
  financial: "Cadastra clientes, fornecedores e os cadastros financeiros; só consulta produtos.",
  sales: "Cadastra clientes e fornecedores; só consulta o resto.",
  stock: "Cadastra produtos e serviços; só consulta o resto.",
  accountant: "Só consulta. Bloqueio de período e exportações chegam depois.",
  viewer: "Só consulta.",
};

/** The roles an invitation can offer: nobody is invited as an owner. */
export const INVITABLE_ROLES: Role[] = ["admin", "financial", "sales", "stock", "accountant", "viewer"];
