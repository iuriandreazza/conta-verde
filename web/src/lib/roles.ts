import type { Role } from "../api";
import { msg } from "../i18n";

/** How a role reads on screen. The keys are the API's and never shown. */
export const ROLE_LABELS: Record<Role, string> = {
  owner: msg("Owner"),
  admin: msg("Administrator"),
  financial: msg("Financial"),
  sales: msg("Sales"),
  stock: msg("Stock"),
  accountant: msg("Accountant"),
  viewer: msg("Read only"),
};

/** What a role can do, in a line, for the person choosing one to invite. */
export const ROLE_HINTS: Record<Role, string> = {
  owner: msg("Everything, and is who created the company."),
  admin: msg("Everything, including inviting people."),
  financial: msg("Registers customers, suppliers and the financial registers; only looks at products."),
  sales: msg("Registers customers and suppliers; only looks at the rest."),
  stock: msg("Registers products and services; only looks at the rest."),
  accountant: msg("Looks at everything but who belongs to the company, and closes and reopens months; changes no registers or entries."),
  viewer: msg("Only looks."),
};

/** The roles a person can be given, by invitation or later: nobody is made an owner. */
export const ASSIGNABLE_ROLES: Role[] = ["admin", "financial", "sales", "stock", "accountant", "viewer"];
