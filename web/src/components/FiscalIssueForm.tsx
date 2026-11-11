import { useEffect, useState, type FormEvent } from "react";
import {
  api,
  errorMessage,
  type BuyerPresence,
  type IssueChoices,
  type OperationNature,
  type PaymentForm,
  type Sale,
} from "../api";
import { useT } from "../i18n";
import { formatMoney } from "../lib/decimal";
import { PAYMENT_LABELS, PRESENCE_LABELS } from "../lib/fiscalDocuments";
import { Button } from "./Button";
import { HelpTitle } from "./HelpTitle";
import { Notice } from "./Notice";

interface Props {
  companyId: string;
  natures: OperationNature[];
  /** The sale is fixed (sending a rejected document again) instead of chosen. */
  fixedSale?: string;
  submitLabel: string;
  onSubmit: (choices: IssueChoices) => Promise<void>;
}

/**
 * Every field here is a tax fact the product does not know, so none starts filled: the person chooses and the first
 * option of each says so. Nothing is sent until the button.
 */
export function IssueForm({ companyId, natures, fixedSale, submitLabel, onSubmit }: Props) {
  const t = useT();
  const [sales, setSales] = useState<Sale[]>([]);
  const [sale, setSale] = useState(fixedSale ?? "");
  const [nature, setNature] = useState("");
  const [origin, setOrigin] = useState("");
  const [presence, setPresence] = useState("");
  const [payment, setPayment] = useState("");
  const [finalConsumer, setFinalConsumer] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    if (fixedSale) return;
    let current = true;
    api
      .listSales(companyId, { q: "", status: "completed", sort: "-number" })
      .then((page) => current && setSales(page.items))
      .catch((failure) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId, fixedSale]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    try {
      await onSubmit({
        sale_id: sale,
        nature_code: nature,
        origin: Number(origin),
        presence: presence as BuyerPresence,
        payment: payment as PaymentForm,
        final_consumer: finalConsumer === "yes",
      });
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setPending(false);
    }
  }

  const productNatures = natures.filter((item) => item.kind === "product" && item.active);

  return (
    <form className="record-form" onSubmit={(event) => void submit(event)}>
      <HelpTitle title={t("How to issue")} level={3}>
        <p>
          {t(
            "These are fiscal choices the system does not know: the operation nature, the origin of the goods (one value for the whole invoice), how the buyer was present, the payment form and whether the buyer is a final consumer. Confirm with your accountant before issuing. Only customers with a CPF get an NF-e for now.",
          )}
        </p>
      </HelpTitle>
      <Notice message={error} />
      <div className="field-grid">
        {!fixedSale && (
          <label>
            {t("Sale")}
            <select required value={sale} onChange={(event) => setSale(event.target.value)}>
              <option value="">{t("Choose the sale")}</option>
              {sales.map((item) => (
                <option key={item.id} value={item.id}>
                  {t("Sale {number} · {total}", { number: item.number, total: formatMoney(item.total) })}
                </option>
              ))}
            </select>
          </label>
        )}
        <label>
          {t("Operation nature")}
          <select required value={nature} onChange={(event) => setNature(event.target.value)}>
            <option value="">{t("Choose the nature")}</option>
            {productNatures.map((item) => (
              <option key={item.code} value={item.code}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("Origin of the goods")}
          <select required value={origin} onChange={(event) => setOrigin(event.target.value)}>
            <option value="">{t("Choose the origin")}</option>
            {[0, 1, 2, 3, 4, 5, 6, 7, 8].map((code) => (
              <option key={code} value={code}>
                {code}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("Buyer presence")}
          <select required value={presence} onChange={(event) => setPresence(event.target.value)}>
            <option value="">{t("Choose the presence")}</option>
            {Object.entries(PRESENCE_LABELS).map(([key, label]) => (
              <option key={key} value={key}>
                {t(label)}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("Payment form")}
          <select required value={payment} onChange={(event) => setPayment(event.target.value)}>
            <option value="">{t("Choose the form")}</option>
            {Object.entries(PAYMENT_LABELS).map(([key, label]) => (
              <option key={key} value={key}>
                {t(label)}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("Final consumer")}
          <select required value={finalConsumer} onChange={(event) => setFinalConsumer(event.target.value)}>
            <option value="">{t("Choose")}</option>
            <option value="yes">{t("Yes")}</option>
            <option value="no">{t("No")}</option>
          </select>
        </label>
      </div>
      <Button type="submit" pending={pending}>
        {submitLabel}
      </Button>
    </form>
  );
}
