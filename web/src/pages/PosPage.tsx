import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Link, useParams } from "react-router-dom";
import {
  api,
  errorMessage,
  type BuyerPresence,
  type FiscalDocument,
  type OperationNature,
  type PaymentForm,
} from "../api";
import { Button } from "../components/Button";
import { HelpTitle } from "../components/HelpTitle";
import { LinesEditor, linesProblem, linesToInput, newLine, type LineRow } from "../components/LinesEditor";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { treeRows } from "../lib/categoryTree";
import { formatMoney } from "../lib/decimal";
import { centsToApi, toCents } from "../lib/entryInstallments";
import { PAYMENT_LABELS, PRESENCE_LABELS, STATUS_LABELS } from "../lib/fiscalDocuments";
import { linesTotalCents } from "../lib/saleLines";
import { todayIso } from "../lib/settlement";
import { useCanWrite } from "../lib/useCanWrite";
import { useSalesTargets } from "../lib/useSalesTargets";

interface PaymentRow {
  key: number;
  form: string;
  amount: string;
}

let lastPayment = 0;
const newPayment = (): PaymentRow => ({ key: ++lastPayment, form: "", amount: "" });

/**
 * The counter: a sale of products made, finished and invoiced as an NFC-e in one go (ADR 0082). It uses the endpoints that
 * already exist, one after the other: the sale, its completion and the NFC-e. Every fiscal choice is the person's and starts
 * empty, what is paid must add up to the total (no change is given), and a sale that was made is never made twice: if the
 * invoice fails, the person tries the invoice again.
 */
export function PosPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("fiscal") && useCanWrite("sales");
  const targets = useSalesTargets(companyId);
  const [natures, setNatures] = useState<OperationNature[]>([]);
  const [customer, setCustomer] = useState("");
  const [category, setCategory] = useState("");
  const [rows, setRows] = useState<LineRow[]>(() => [newLine()]);
  const [payments, setPayments] = useState<PaymentRow[]>(() => [newPayment()]);
  const [nature, setNature] = useState("");
  const [origin, setOrigin] = useState("");
  const [presence, setPresence] = useState("");
  const [identify, setIdentify] = useState("");
  const [saleId, setSaleId] = useState<string | null>(null);
  const [made, setMade] = useState<FiscalDocument | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    api
      .getFiscalSettings(companyId)
      .then((settings) => setNatures(settings?.natures ?? []))
      .catch(() => setNatures([]));
  }, [companyId]);

  const total = linesTotalCents(rows.filter((row) => row.product_id !== "").map((row) => ({ quantity: row.quantity, unitPrice: row.unit_price, discount: row.discount })));
  const paid = useMemo(() => {
    let sum = 0n;
    for (const row of payments) {
      if (row.amount.trim() === "") continue;
      const cents = toCents(row.amount);
      if (cents === null) return null;
      sum += cents;
    }
    return sum;
  }, [payments]);
  const balanced = total !== null && total > 0n && paid === total;

  const customers = targets.parties.filter((party) => party.is_customer);
  const products = targets.products.filter((product) => product.active && product.kind === "product");
  const categoryRows = treeRows(targets.categories).filter(({ category: item }) => item.active && item.dre_group !== "transfers");
  const productNatures = natures.filter((item) => item.kind === "product" && item.active);

  async function finish(event: FormEvent) {
    event.preventDefault();
    if (pending || total === null) return;
    const problem = linesProblem(rows);
    if (problem !== null) {
      setError(problem);
      return;
    }
    setPending(true);
    setError(null);
    try {
      // The sale is made and completed once: a failure of the invoice leaves it, and the next try only asks for the invoice.
      let sale = saleId;
      if (sale === null) {
        const created = await api.createSale(companyId, {
          customer_id: customer,
          sold_on: todayIso(),
          category_id: category,
          lines: linesToInput(rows),
        });
        sale = created.id;
        setSaleId(sale);
        await api.completeSale(companyId, sale);
      }
      const document = await api.issueConsumerDocument(companyId, {
        sale_id: sale,
        nature_code: nature,
        origin: Number(origin),
        presence: presence as BuyerPresence,
        identify_consumer: identify === "yes",
        payments: payments
          .filter((row) => row.amount.trim() !== "")
          .map((row) => ({ form: row.form as PaymentForm, amount: centsToApi(toCents(row.amount) ?? 0n) })),
      });
      setMade(document);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setPending(false);
    }
  }

  function startOver() {
    setRows([newLine()]);
    setPayments([newPayment()]);
    setSaleId(null);
    setMade(null);
    setError(null);
    setIdentify("");
  }

  const changePayment = (key: number, patch: Partial<PaymentRow>) =>
    setPayments((current) => current.map((row) => (row.key === key ? { ...row, ...patch } : row)));

  if (made) {
    return (
      <>
        <PageHeader title="Caixa" />
        <div className="record-detail">
          <p role="status">
            NFC-e: <span className="badge">{STATUS_LABELS[made.status]}</span> · {formatMoney(made.total)}
          </p>
          {made.access_key && <p className="access-key">{made.access_key}</p>}
          {made.refusal && <Notice message={`Recusada (${made.refusal.code}): ${made.refusal.message}`} tone="warning" />}
          {made.environment === "homologation" && <Notice message="Documento de homologação: não tem valor fiscal." tone="warning" />}
          <div className="form-actions">
            <Button type="button" onClick={startOver}>
              Nova venda
            </Button>
            <Link to={`/companies/${companyId}/fiscal/documents`}>Ver nos documentos fiscais</Link>
          </div>
        </div>
      </>
    );
  }

  return (
    <>
      <PageHeader
        title="Caixa"
        description="Uma venda de produtos no balcão, concluída e emitida como NFC-e de uma vez. As escolhas fiscais são suas e começam vazias; o que foi pago precisa somar o total da venda, porque o sistema não declara troco."
      />
      {!canWrite ? (
        <Notice message="Só quem escreve em Vendas e em Fiscal usa o caixa." />
      ) : (
        <form className="record-form" onSubmit={(event) => void finish(event)}>
          <Notice message={error ?? targets.error} />
          {saleId !== null && (
            <Notice tone="warning" message="A venda já foi feita e concluída. Corrija o que a NFC-e pede e tente emitir de novo: a venda não é refeita." />
          )}
          <div className="field-grid">
            <label>
              Cliente
              <select required value={customer} onChange={(event) => setCustomer(event.target.value)} disabled={saleId !== null}>
                <option value="">Escolha o cliente</option>
                {customers.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Categoria da receita
              <select required value={category} onChange={(event) => setCategory(event.target.value)} disabled={saleId !== null}>
                <option value="">Escolha a categoria</option>
                {categoryRows.map(({ category: item, depth }) => (
                  <option key={item.id} value={item.id}>
                    {`${"— ".repeat(depth)}${item.name}`}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <LinesEditor companyId={companyId} rows={rows} onChange={setRows} products={products} />

          <h3>Pagamento</h3>
          {payments.map((row) => (
            <div className="field-grid" key={row.key}>
              <label>
                Forma de pagamento
                <select required value={row.form} onChange={(event) => changePayment(row.key, { form: event.target.value })}>
                  <option value="">Escolha a forma</option>
                  {Object.entries(PAYMENT_LABELS).map(([key, label]) => (
                    <option key={key} value={key}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Valor pago
                <input required inputMode="decimal" value={row.amount} onChange={(event) => changePayment(row.key, { amount: event.target.value })} />
              </label>
            </div>
          ))}
          <div className="form-actions">
            <Button type="button" onClick={() => setPayments((current) => [...current, newPayment()])}>
              Outra forma de pagamento
            </Button>
          </div>
          <p role="status">
            Total {total === null ? "—" : formatMoney(centsToApi(total))} · Pago {paid === null ? "—" : formatMoney(centsToApi(paid))}
            {total !== null && paid !== null && total !== paid ? ` · Falta ${formatMoney(centsToApi(total > paid ? total - paid : paid - total))} ${total > paid ? "a receber" : "a menos"}` : ""}
          </p>

          <HelpTitle title="Escolhas fiscais" level={3}>
            <p>
              São fatos fiscais que o sistema não sabe: a natureza de operação, a origem da mercadoria, como o comprador estava presente
              e se o consumidor é identificado na nota (com o CPF ou CNPJ do cliente). A NFC-e sai das linhas de produto, sempre dentro
              do estado. Confirme com o seu contador.
            </p>
          </HelpTitle>
          <div className="field-grid">
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
              Identificar o consumidor na nota
              <select required value={identify} onChange={(event) => setIdentify(event.target.value)}>
                <option value="">Escolha</option>
                <option value="yes">Sim</option>
                <option value="no">Não</option>
              </select>
            </label>
          </div>
          <Button type="submit" pending={pending} disabled={!balanced}>
            {saleId === null ? "Finalizar e emitir NFC-e" : "Emitir a NFC-e de novo"}
          </Button>
        </form>
      )}
    </>
  );
}
