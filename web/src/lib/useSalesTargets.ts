import { useEffect, useMemo, useState } from "react";
import {
  NO_FILTERS,
  NO_PAYMENT_TERM_FILTERS,
  NO_PRODUCT_FILTERS,
  api,
  errorMessage,
  type Category,
  type CostCenter,
  type Party,
  type PaymentTerm,
  type Product,
} from "../api";
import { readUpTo } from "./readUpTo";

/** What a contract or a quote points at, read once for a company; the list pages and the "new" sheets need it. */
export function useSalesTargets(companyId: string) {
  const [parties, setParties] = useState<Party[]>([]);
  const [products, setProducts] = useState<Product[]>([]);
  const [terms, setTerms] = useState<PaymentTerm[]>([]);
  const [categories, setCategories] = useState<Category[]>([]);
  const [costCenters, setCostCenters] = useState<CostCenter[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let current = true;
    Promise.all([
      readUpTo((after) => api.listParties(companyId, NO_FILTERS, after)),
      readUpTo((after) => api.listProducts(companyId, NO_PRODUCT_FILTERS, after)),
      readUpTo((after) => api.listPaymentTerms(companyId, NO_PAYMENT_TERM_FILTERS, after)),
      api.listCategories(companyId),
      readUpTo((after) => api.listCostCenters(companyId, { q: "", active: "", sort: "name" }, after)),
    ])
      .then(([partyList, productList, termList, categoryList, centerList]) => {
        if (current) {
          setParties(partyList);
          setProducts(productList);
          setTerms(termList);
          setCategories(categoryList.items);
          setCostCenters(centerList);
          setLoaded(true);
        }
      })
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  const names = useMemo(() => {
    const byId = new Map<string, string>();
    for (const item of [...parties, ...terms, ...products, ...categories, ...costCenters]) {
      byId.set(item.id, item.name);
    }
    return byId;
  }, [parties, terms, products, categories, costCenters]);

  return { parties, products, terms, categories, costCenters, names, error, loaded };
}
