import { useEffect, useState, type ReactNode } from "react";
import { api, errorMessage, type CostCenterInput, type PaymentMethodInput, type PaymentTermInput } from "../api";
import { CostCenterForm } from "../components/CostCenterForm";
import { PaymentMethodForm } from "../components/PaymentMethodForm";
import { PaymentTermForm } from "../components/PaymentTermForm";
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
  message: (created: Created) => string;
  form: (props: FormProps<Input>) => ReactNode;
}) {
  return function RegisterCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
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
        onCreated({ id: created.id, message: options.message(created) });
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
  message: (created) => `${created.name} cadastrado.`,
  form: (props) => <CostCenterForm costCenter={null} {...props} />,
});

export const PaymentMethodCreator = registerCreator<PaymentMethodInput, { id: string; name: string }>({
  create: (companyId, input) => api.createPaymentMethod(companyId, input),
  message: (created) => `${created.name} cadastrada.`,
  form: (props) => <PaymentMethodForm method={null} {...props} />,
});

export const PaymentTermCreator = registerCreator<PaymentTermInput, { id: string; name: string }>({
  create: (companyId, input) => api.createPaymentTerm(companyId, input),
  message: (created) => `${created.name} cadastrado.`,
  form: (props) => <PaymentTermForm term={null} {...props} />,
});
