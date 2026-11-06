import { useEffect, useState } from "react";
import { api, errorMessage, type PartyInput } from "../api";
import { PartyForm } from "../components/PartyForm";
import { registryNoticeFor } from "../lib/registryNotice";
import type { CreatorProps } from "./CreationProvider";

/** The form of a new client or supplier, and the call that saves it; what the registry said comes back in the confirmation. */
export function PartyCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: PartyInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createParty(companyId, input);
      const notice = registryNoticeFor(created, "registering");
      onCreated({
        id: created.id,
        message: notice?.message ?? `${created.name} cadastrado.`,
        tone: notice?.tone === "warning" ? "warning" : "success",
      });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  return (
    <PartyForm
      party={null}
      focusName={false}
      pending={saving}
      blocked={false}
      error={error}
      notice={null}
      lookupAddress={(postalCode) => api.lookupPostalCode(companyId, postalCode)}
      onSubmit={(input) => void save(input)}
      onCancel={onCancel}
    />
  );
}
