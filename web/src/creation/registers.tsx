import { useEffect, useState, type ReactNode } from "react";
import { api, errorMessage, type BankAccountInput, type CommissionRuleInput, type Party, type PriceListInput, NO_FILTERS, type Product, NO_PRODUCT_FILTERS, type CostCenterInput, type PaymentMethodInput, type PaymentTermInput } from "../api";
import { BankAccountForm } from "../components/BankAccountForm";
import { CostCenterForm } from "../components/CostCenterForm";
import { PaymentMethodForm } from "../components/PaymentMethodForm";
import { PaymentTermForm } from "../components/PaymentTermForm";
import { CommissionRuleForm } from "../components/CommissionRuleForm";
import { PriceListForm } from "../components/PriceListForm";
import { useT, type Translate } from "../i18n";
import { Spinner } from "../components/Spinner";
import { formatDecimal } from "../lib/decimal";
import { readUpTo } from "../lib/readUpTo";
import type { CreatorProps } from "./CreationProvider";

interface FormProps<Input> {
  pending: boolean;
  error: string | null;
  onSubmit: (input: Input) => void;
  onCancel: () => void;
}

/**
 * A creator for a register whose form needs nothing read first: it holds the saving state, tells the
 * host a save is on its way, and reports the new record with the words that confirm it.
 */
function registerCreator<Input, Created extends { id: string }>(options: {
  create: (companyId: string, input: Input) => Promise<Created>;
  message: (created: Created, t: Translate) => string;
  form: (props: FormProps<Input>) => ReactNode;
}) {
  return function RegisterCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
    const t = useT();
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
      onBusyChange(saving);
    }, [saving, onBusyChange]);

    async function save(input: Input) {
      setSaving(true);
      setError(null);
      try {
        const created = await options.create(companyId, input);
        onCreated({ id: created.id, message: options.message(created, t) });
      } catch (failure) {
        setError(errorMessage(failure));
        setSaving(false);
      }
    }

    return options.form({ pending: saving, error, onSubmit: (input) => void save(input), onCancel });
  };
}

export const CostCenterCreator = registerCreator<CostCenterInput, { id: string; name: string }>({
  create: (companyId, input) => api.createCostCenter(companyId, input),
  message: (created, t) => t("{name} registered.", { name: created.name }),
  form: (props) => <CostCenterForm costCenter={null} {...props} />,
});

export const PaymentMethodCreator = registerCreator<PaymentMethodInput, { id: string; name: string }>({
  create: (companyId, input) => api.createPaymentMethod(companyId, input),
  message: (created, t) => t("Payment method {name} registered.", { name: created.name }),
  form: (props) => <PaymentMethodForm method={null} {...props} />,
});

export const PaymentTermCreator = registerCreator<PaymentTermInput, { id: string; name: string }>({
  create: (companyId, input) => api.createPaymentTerm(companyId, input),
  message: (created, t) => t("{name} registered.", { name: created.name }),
  form: (props) => <PaymentTermForm term={null} {...props} />,
});

export const BankAccountCreator = registerCreator<BankAccountInput, { id: string; name: string }>({
  create: (companyId, input) => api.createBankAccount(companyId, input),
  message: (created, t) => t("Bank account {name} registered.", { name: created.name }),
  form: (props) => <BankAccountForm account={null} {...props} />,
});

/** The form of a new price table, with the products read first to choose from. */
export function PriceListCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const [products, setProducts] = useState<Product[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    readUpTo((after) => api.listProducts(companyId, NO_PRODUCT_FILTERS, after))
      .then((list) => current && setProducts(list))
      .catch((failure: unknown) => current && setLoadError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: PriceListInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createPriceList(companyId, input);
      onCreated({ id: created.id, message: `${created.name} cadastrada.` });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  if (products === null) {
    return loadError === null ? (
      <p role="status" className="status">
        <Spinner /> Carregando…
      </p>
    ) : (
      <p role="alert">{loadError}</p>
    );
  }
  return <PriceListForm list={null} products={products} pending={saving} error={error} onSubmit={(input) => void save(input)} onCancel={onCancel} />;
}

/** The form of a new commission rule, with the parties and products read first to choose from. */
export function CommissionRuleCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const [targets, setTargets] = useState<{ parties: Party[]; products: Product[] } | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    Promise.all([
      readUpTo((after) => api.listParties(companyId, NO_FILTERS, after)),
      readUpTo((after) => api.listProducts(companyId, NO_PRODUCT_FILTERS, after)),
    ])
      .then(([parties, products]) => current && setTargets({ parties, products }))
      .catch((failure: unknown) => current && setLoadError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: CommissionRuleInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createCommissionRule(companyId, input);
      onCreated({ id: created.id, message: `Regra de ${formatDecimal(created.rate)}% cadastrada.` });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  if (targets === null) {
    return loadError === null ? (
      <p role="status" className="status">
        <Spinner /> Carregando…
      </p>
    ) : (
      <p role="alert">{loadError}</p>
    );
  }
  return (
    <CommissionRuleForm rule={null} parties={targets.parties} products={targets.products} pending={saving} error={error} onSubmit={(input) => void save(input)} onCancel={onCancel} />
  );
}
