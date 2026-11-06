import { useEffect, useState } from "react";
import { api, errorMessage, type BankAccount, type TransferInput } from "../api";
import { Spinner } from "../components/Spinner";
import { TransferForm } from "../components/TransferForm";
import { formatMoney } from "../lib/decimal";
import { readUpTo } from "../lib/readUpTo";
import type { CreatorProps } from "./CreationProvider";

/** The form of a new transfer, with the company's accounts read first, and the call that saves it. */
export function TransferCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const [accounts, setAccounts] = useState<BankAccount[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    readUpTo((after) => api.listBankAccounts(companyId, undefined, after))
      .then((list) => current && setAccounts(list.filter((account) => account.active)))
      .catch((failure: unknown) => current && setLoadError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: TransferInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createTransfer(companyId, input);
      onCreated({ id: created.id, message: `Transferência de ${formatMoney(created.amount)} registrada.` });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  if (accounts === null) {
    return loadError === null ? (
      <p role="status" className="status">
        <Spinner /> Carregando…
      </p>
    ) : (
      <p role="alert">{loadError}</p>
    );
  }
  return (
    <TransferForm accounts={accounts} pending={saving} error={error} onSubmit={(input) => void save(input)} onCancel={onCancel} />
  );
}
