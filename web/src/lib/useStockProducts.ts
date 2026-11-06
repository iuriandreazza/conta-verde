import { useEffect, useState } from "react";
import { NO_PRODUCT_FILTERS, api, errorMessage, type Product } from "../api";
import { readUpTo } from "./readUpTo";

/** The products of a company (only those have stock, services do not), read once for the stock screens and their sheet. */
export function useStockProducts(companyId: string) {
  const [products, setProducts] = useState<Product[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let current = true;
    readUpTo((after) => api.listProducts(companyId, { ...NO_PRODUCT_FILTERS, kind: "product" }, after))
      .then((list) => {
        if (current) {
          setProducts(list);
          setLoaded(true);
        }
      })
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  return { products, error, loaded };
}
