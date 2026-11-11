import { useCallback } from "react";
import { api } from "../api";
import { msg, useT, type Translate } from "../i18n";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  name: msg("Name"),
  document: msg("Document"),
  email: msg("E-mail"),
  is_customer: msg("Customer"),
  is_supplier: msg("Supplier"),
  is_carrier: msg("Carrier"),
  is_seller: msg("Seller"),
  postal_code: msg("Postal code"),
  street: msg("Street"),
  street_number: msg("Number"),
  complement: msg("Complement"),
  district: msg("District"),
  city: msg("City"),
  state: msg("Address state"),
  tags: msg("Tags"),
  contacts: msg("Contacts"),
  document_check: msg("CNPJ check"),
  registry_legal_name: msg("Legal name (federal registry)"),
  registry_situation: msg("Status (federal registry)"),
};

const ROLE_FIELDS = ["is_customer", "is_supplier", "is_carrier", "is_seller"];

const CHECK_LABELS: Record<string, string> = {
  verified: msg("Verified"),
  not_found: msg("Not found"),
  unverified: msg("Not verified"),
};

function label(field: string, t: Translate): string {
  const text = FIELD_LABELS[field];
  return text === undefined ? field : t(text);
}

function display(field: string, value: string | null, t: Translate): string {
  if (value === null) return "—";
  if (ROLE_FIELDS.includes(field)) return value === "true" ? t("yes") : t("no");
  if (field === "document_check") return value in CHECK_LABELS ? t(CHECK_LABELS[value]) : value;
  return value;
}

export function PartyHistory({ companyId, partyId }: { companyId: string; partyId: string }) {
  const t = useT();
  const load = useCallback(() => api.partyHistory(companyId, partyId), [companyId, partyId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => label(field, t)}
      valueLabel={(field, value) => display(field, value, t)}
      // Parties created before the audit trail existed have no entries until they change.
      emptyMessage={t("No records: this record predates the history. The next changes will appear here.")}
    />
  );
}
