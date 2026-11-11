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
import { useI18n } from "../i18n";
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
  const { t, rich } = useI18n();
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
  // The sale as it stands on the server: made, then completed. A step that failed is the one asked again, never the ones before.
  const [saleId, setSaleId] = useState<string | null>(null);
  const [completed, setCompleted] = useState(false);
  // The invoice the authority refused: it is sent again under its own id, never issued as a second one.
  const [refused, setRefused] = useState<FiscalDocument | null>(null);
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
      // A payment of nothing is not one: the invoice refuses it.
      if (cents === null || cents === 0n) return null;
      sum += cents;
    }
    return sum;
  }, [payments]);
  const balanced = total !== null && total > 0n && paid === total;

  const customers = targets.parties.filter((party) => party.is_customer);
  const products = targets.products.filter((product) => product.active && product.kind === "product");
  const categoryRows = treeRows(targets.categories).filter(({ category: item }) => item.active && item.dre_group !== "transfers");
  const productNatures = natures.filter((item) => item.kind === "product" && item.active);
  // The buyer is at the counter: the presences an NFC-e takes.
  const counterPresences = Object.entries(PRESENCE_LABELS).filter(([key]) =>
    ["in_person", "home_delivery", "outside_the_establishment", "other"].includes(key),
  );

  async function finish(event: FormEvent) {
    event.preventDefault();
    if (pending || total === null) return;
    const problem = linesProblem(rows, t);
    if (problem !== null) {
      setError(problem);
      return;
    }
    setPending(true);
    setError(null);
    try {
      // The sale is made once, completed once and invoiced until the invoice is authorized: whatever failed is what is asked again.
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
      }
      if (!completed) {
        await api.completeSale(companyId, sale);
        setCompleted(true);
      }
      const choices = {
        sale_id: sale,
        nature_code: nature,
        origin: Number(origin),
        presence: presence as BuyerPresence,
        identify_consumer: identify === "yes",
        payments: payments
          .filter((row) => row.amount.trim() !== "")
          .map((row) => ({ form: row.form as PaymentForm, amount: centsToApi(toCents(row.amount) ?? 0n) })),
      };
      const document = refused
        ? await api.resendConsumerDocument(companyId, refused.id, choices)
        : await api.issueConsumerDocument(companyId, choices);
      if (document.status === "rejected") {
        // The sale stands and the invoice is asked again under the same id: the person fixes what was refused first.
        setRefused(document);
        setError(t("Refused ({code}): {message}", { code: document.refusal?.code ?? "?", message: document.refusal?.message ?? "" }));
      } else {
        setRefused(null);
        setMade(document);
      }
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
    setCompleted(false);
    setRefused(null);
    setMade(null);
    setError(null);
    setIdentify("");
  }

  const changePayment = (key: number, patch: Partial<PaymentRow>) =>
    setPayments((current) => current.map((row) => (row.key === key ? { ...row, ...patch } : row)));

  if (made) {
    return (
      <>
        <PageHeader title={t("Point of sale")} />
        <div className="record-detail">
          <p role="status">
            {rich("NFC-e: {status} · {amount}", {
              status: <span className="badge">{t(STATUS_LABELS[made.status])}</span>,
              amount: formatMoney(made.total),
            })}
          </p>
          {made.access_key && <p className="access-key">{made.access_key}</p>}
          {made.refusal && <Notice message={t("Refused ({code}): {message}", { code: made.refusal.code, message: made.refusal.message })} tone="warning" />}
          {made.environment === "homologation" && <Notice message={t("Homologation document: it has no fiscal value.")} tone="warning" />}
          <div className="form-actions">
            <Button type="button" onClick={startOver}>
              {t("New sale")}
            </Button>
            <Link to={`/companies/${companyId}/fiscal/documents`}>{t("See in the fiscal documents")}</Link>
          </div>
        </div>
      </>
    );
  }

  return (
    <>
      <PageHeader
        title={t("Point of sale")}
        description={t("A sale of products at the counter, completed and issued as an NFC-e in one go. The fiscal choices are yours and start empty; what was paid must add up to the sale total, because the system does not declare change.")}
      />
      {!canWrite ? (
        <Notice message={t("Only those who write in Sales and in Fiscal use the point of sale.")} />
      ) : (
        <form className="record-form" onSubmit={(event) => void finish(event)}>
          <Notice message={error ?? targets.error} />
          {saleId !== null && (
            <Notice
              tone="warning"
              message={
                completed
                  ? t("The sale was already made and completed. Fix what the NFC-e asks for and issue it again: the sale is not made again.")
                  : t("The sale was made, but not yet completed. Try again: it is not made again.")
              }
            />
          )}
          <div className="field-grid">
            <label>
              {t("Customer")}
              <select required value={customer} onChange={(event) => setCustomer(event.target.value)} disabled={saleId !== null}>
                <option value="">{t("Choose the customer")}</option>
                {customers.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("Revenue category")}
              <select required value={category} onChange={(event) => setCategory(event.target.value)} disabled={saleId !== null}>
                <option value="">{t("Choose the category")}</option>
                {categoryRows.map(({ category: item, depth }) => (
                  <option key={item.id} value={item.id}>
                    {`${"— ".repeat(depth)}${item.name}`}
                  </option>
                ))}
              </select>
            </label>
          </div>
          {/* Once the sale exists its lines are the server's: they are not edited here, and the total the payment must match is theirs. */}
          <fieldset disabled={saleId !== null} className="fieldset-plain">
            <LinesEditor companyId={companyId} rows={rows} onChange={setRows} products={products} />
          </fieldset>

          <h3>{t("Payment")}</h3>
          {payments.map((row) => (
            <div className="field-grid" key={row.key}>
              <label>
                {t("Payment method")}
                <select required value={row.form} onChange={(event) => changePayment(row.key, { form: event.target.value })}>
                  <option value="">{t("Choose the method")}</option>
                  {Object.entries(PAYMENT_LABELS).map(([key, label]) => (
                    <option key={key} value={key}>
                      {t(label)}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                {t("Amount paid")}
                <input required inputMode="decimal" value={row.amount} onChange={(event) => changePayment(row.key, { amount: event.target.value })} />
              </label>
            </div>
          ))}
          <div className="form-actions">
            <Button type="button" onClick={() => setPayments((current) => [...current, newPayment()])}>
              {t("Another payment method")}
            </Button>
          </div>
          <p role="status">
            {t("Total {total} · Paid {paid}", {
              total: total === null ? "—" : formatMoney(centsToApi(total)),
              paid: paid === null ? "—" : formatMoney(centsToApi(paid)),
            })}
            {total !== null && paid !== null && total !== paid
              ? ` · ${
                  total > paid
                    ? t("Missing {amount} to receive", { amount: formatMoney(centsToApi(total - paid)) })
                    : t("Missing {amount} less", { amount: formatMoney(centsToApi(paid - total)) })
                }`
              : ""}
          </p>

          <HelpTitle title={t("Fiscal choices")} level={3}>
            <p>
              {t("These are fiscal facts the system does not know: the operation nature, the origin of the goods, how the buyer was present and whether the consumer is identified on the invoice (with the customer's CPF or CNPJ). The NFC-e comes from the product lines, always within the state. Confirm with your accountant.")}
            </p>
          </HelpTitle>
          <div className="field-grid">
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
                {counterPresences.map(([key, label]) => (
                  <option key={key} value={key}>
                    {t(label)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("Identify the consumer on the invoice")}
              <select required value={identify} onChange={(event) => setIdentify(event.target.value)}>
                <option value="">{t("Choose")}</option>
                <option value="yes">{t("Yes")}</option>
                <option value="no">{t("No")}</option>
              </select>
            </label>
          </div>
          <Button type="submit" pending={pending} disabled={!balanced}>
            {saleId === null ? t("Finish and issue NFC-e") : completed ? t("Issue the NFC-e again") : t("Complete the sale and issue")}
          </Button>
        </form>
      )}
    </>
  );
}
