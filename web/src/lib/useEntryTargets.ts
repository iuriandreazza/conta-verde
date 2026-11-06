import { useEffect, useMemo, useState } from "react";
import { api, errorMessage, type BankAccount, type Category, type CostCenter, type PaymentMethod } from "../api";
import { readUpTo } from "./readUpTo";

/**
 * What an entry (or a recurrence) points at, read once for a company: the categories, cost centers and
 * parties, and, unless `payments` is false, the bank accounts and payment methods that settling an entry uses.
 */
export function useEntryTargets(companyId: string, { payments = true }: { payments?: boolean } = {}) {
  const [banks, setBanks] = useState<BankAccount[]>([]);
  const [methods, setMethods] = useState<PaymentMethod[]>([]);
  const [categories, setCategories] = useState<Category[]>([]);
  const [costCenters, setCostCenters] = useState<CostCenter[]>([]);
  const [parties, setParties] = useState<{ id: string; name: string }[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let current = true;
    Promise.all([
      api.listCategories(companyId),
      readUpTo((after) => api.listCostCenters(companyId, { q: "", active: "", sort: "name" }, after)),
      readUpTo((after) => api.listParties(companyId, undefined, after)),
      payments ? readUpTo((after) => api.listBankAccounts(companyId, undefined, after)) : Promise.resolve([]),
      payments ? readUpTo((after) => api.listPaymentMethods(companyId, undefined, after)) : Promise.resolve([]),
    ])
      .then(([categoryList, costCenterList, partyList, bankList, methodList]) => {
        if (current) {
          setCategories(categoryList.items);
          setBanks(bankList);
          setMethods(methodList);
          setCostCenters(costCenterList);
          setParties(partyList.map(({ id, name }) => ({ id, name })));
          setLoaded(true);
        }
      })
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId, payments]);

  const names = useMemo(() => {
    const byId = new Map<string, string>();
    for (const category of categories) byId.set(category.id, category.name);
    for (const center of costCenters) byId.set(center.id, center.name);
    for (const party of parties) byId.set(party.id, party.name);
    return byId;
  }, [categories, costCenters, parties]);

  return { banks, methods, categories, costCenters, parties, names, error, loaded };
}
