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
      <HelpTitle title="Como emitir" level={3}>
        <p>
          São escolhas fiscais que o sistema não sabe: a natureza de operação, a origem da mercadoria (um valor para a nota
          inteira), como o comprador estava presente, a forma de pagamento e se é consumidor final. Confirme com o seu contador
          antes de emitir. Só clientes com CPF recebem NF-e por enquanto.
        </p>
      </HelpTitle>
      <Notice message={error} />
      <div className="field-grid">
        {!fixedSale && (
          <label>
            Venda
            <select required value={sale} onChange={(event) => setSale(event.target.value)}>
              <option value="">Escolha a venda</option>
              {sales.map((item) => (
                <option key={item.id} value={item.id}>
                  {`Venda ${item.number} · ${formatMoney(item.total)}`}
                </option>
              ))}
            </select>
          </label>
        )}
        <label>
          Natureza de operação
          <select required value={nature} onChange={(event) => setNature(event.target.value)}>
            <option value="">Escolha a natureza</option>
            {productNatures.map((item) => (
              <option key={item.code} value={item.code}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Origem da mercadoria
          <select required value={origin} onChange={(event) => setOrigin(event.target.value)}>
            <option value="">Escolha a origem</option>
            {[0, 1, 2, 3, 4, 5, 6, 7, 8].map((code) => (
              <option key={code} value={code}>
                {code}
              </option>
            ))}
          </select>
        </label>
        <label>
          Presença do comprador
          <select required value={presence} onChange={(event) => setPresence(event.target.value)}>
            <option value="">Escolha a presença</option>
            {Object.entries(PRESENCE_LABELS).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Forma de pagamento
          <select required value={payment} onChange={(event) => setPayment(event.target.value)}>
            <option value="">Escolha a forma</option>
            {Object.entries(PAYMENT_LABELS).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Consumidor final
          <select required value={finalConsumer} onChange={(event) => setFinalConsumer(event.target.value)}>
            <option value="">Escolha</option>
            <option value="yes">Sim</option>
            <option value="no">Não</option>
          </select>
        </label>
      </div>
      <Button type="submit" pending={pending}>
        {submitLabel}
      </Button>
    </form>
  );
}
