import { useEffect, useState, type DragEvent, type FormEvent } from "react";
import { Link } from "react-router-dom";
import type { Category, CostCenter, InvoiceEntryParams, InvoicePreview, PaymentTerm, Product } from "../api";
import { msg, useI18n, useT, type Translate } from "../i18n";
import { treeRows } from "../lib/categoryTree";
import { formatDecimal, formatMoney } from "../lib/decimal";
import { formatDate, wholeDate } from "../lib/entryInstallments";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { Notice } from "./Notice";
import { Stepper } from "./Stepper";
import { useSheetDirty } from "./Sheet";
import { HelpTitle } from "./HelpTitle";

/** The most the API reads of an invoice: a bigger file is refused before it is sent. */
export const MAX_INVOICE_BYTES = 1024 * 1024;

/** Why the file cannot be an invoice, or `null` when the server may decide. */
export function invoiceFileProblem(file: File, t: Translate): string | null {
  if (!file.name.toLowerCase().endsWith(".xml")) return t("Choose the invoice's XML file (.xml).");
  if (file.size > MAX_INVOICE_BYTES) return t("The file is larger than 1 MiB: an NF-e is not that big. Check that it is the right XML.");
  return null;
}

/** What the sheet loaded for the company: where the invoice's payables and items can be put. */
export interface PurchaseImportTargets {
  products: Product[];
  terms: PaymentTerm[];
  categories: Category[];
  costCenters: CostCenter[];
}

interface PurchaseImportFormProps {
  companyId: string;
  targets: PurchaseImportTargets;
  /** What the server read from the file; `null` while the file is still to be chosen. */
  preview: InvoicePreview | null;
  /** Reading the file, or entering the invoice, is on its way. */
  pending: boolean;
  error: string | null;
  onRead: (file: File) => void;
  onEnter: (params: InvoiceEntryParams) => void;
  /** Goes back to choosing the file. */
  onChangeFile: () => void;
  onCancel: () => void;
}

/**
 * Enters a supplier's NF-e as a purchase in five steps ([ADR 0057]): the file, what the invoice says, the nature of the
 * purchase (where the payables go), the products of the items, and a last look before it is entered.
 */
const STEPS = [msg("XML file"), msg("Invoice details"), msg("Nature of the purchase"), msg("Products"), msg("Finish")];

/** Enters a supplier's NF-e as a purchase, step by step. */
export function PurchaseImportForm(props: PurchaseImportFormProps) {
  return props.preview === null ? <ChooseFile {...props} /> : <EnterInvoice {...props} preview={props.preview} />;
}

function ChooseFile({ pending, error, onRead, onCancel }: PurchaseImportFormProps) {
  const t = useT();
  const [file, setFile] = useState<File | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  // Something is being dragged over the box: it says where to let go.
  const [dragging, setDragging] = useState(false);

  function choose(chosen: File | null) {
    setFile(chosen);
    setProblem(chosen === null ? null : invoiceFileProblem(chosen, t));
  }

  function drop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    setDragging(false);
    if (!pending) choose(event.dataTransfer.files[0] ?? null);
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (file !== null && problem === null && !pending) onRead(file);
  }

  return (
    <form className="record-form" onSubmit={submit}>
      <Stepper steps={STEPS.map((name) => t(name))} current={0} />
      <section className="form-section" aria-labelledby="import-file">
        <HelpTitle id="import-file" title={t("Invoice file")}>
          <p>
            {t("The NF-e is saved on your computer as an XML file. When you read the file you see the invoice, the supplier and the products before entering the purchase.")}
          </p>
        </HelpTitle>
        <div
          className={dragging ? "dropzone dropzone-active" : "dropzone"}
          data-testid="invoice-dropzone"
          onDragOver={(event) => {
            event.preventDefault();
            setDragging(true);
          }}
          onDragLeave={() => setDragging(false)}
          onDrop={drop}
        >
          <label>
            {t("Choose a file")}
            <input
              type="file"
              aria-label={t("NF-e XML file")}
              accept=".xml,application/xml,text/xml"
              disabled={pending}
              onChange={(event) => choose(event.target.files?.[0] ?? null)}
            />
          </label>
          <p className="muted">{t("Or drag the XML into this area")}</p>
          {file !== null && <p className="dropzone-file">{file.name}</p>}
        </div>
      </section>
      <Notice message={problem ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" pending={pending} disabled={file === null || problem !== null}>
          {t("Read file")}
        </Button>
      </div>
    </form>
  );
}

function EnterInvoice({ companyId, targets, preview, pending, error, onEnter, onChangeFile, onCancel }: PurchaseImportFormProps & { preview: InvoicePreview }) {
  const { t, rich } = useI18n();
  // Step 1 of STEPS is the file, which is behind us: the first one here is index 1.
  const [step, setStep] = useState(1);
  const [chosen, setChosen] = useState<string[]>(() => preview.items.map((item) => item.product_id ?? ""));
  const [categoryId, setCategoryId] = useState("");
  const [costCenterId, setCostCenterId] = useState("");
  const [termId, setTermId] = useState("");
  const [receivedOn, setReceivedOn] = useState(preview.issued_on);
  // What the API said about the last attempt is about the values of that attempt: once the person changes one or goes to
  // another step it is put away, and a new attempt (the parent clears it first) shows its own.
  const [putAway, setPutAway] = useState<string | null>(null);
  useEffect(() => {
    if (error === null) setPutAway(null);
  }, [error]);
  const markDirty = useSheetDirty();
  const categoryRows = treeRows(targets.categories).filter(({ category }) => category.active && category.dre_group !== "transfers");
  const products = targets.products.filter((product) => product.active || chosen.includes(product.id));
  const withoutProduct = chosen.filter((id) => id === "").length;
  const unregistered = preview.supplier_id === null;
  const alreadyEntered = preview.entered?.status === "received";
  const unfinished = preview.entered !== null && !alreadyEntered;
  const hasInstallments = preview.installments.length > 0;
  const category = categoryRows.find(({ category: row }) => row.id === categoryId)?.category;
  const term = targets.terms.find((item) => item.id === termId);

  // What lets each step go on: the rest is chosen on the steps after it.
  const stepProblem: Record<number, string | null> = {
    1: unregistered ? t("Register the supplier to continue.") : alreadyEntered ? t("This invoice has already been entered.") : null,
    2:
      categoryId === ""
        ? t("Choose the category of the payables.")
        : wholeDate(receivedOn) === ""
          ? t("Enter the day the goods arrived.")
          : null,
    3: withoutProduct > 0 ? t("Choose the product of each item.") : null,
  };

  function change<T>(setter: (value: T) => void, value: T) {
    markDirty();
    setPutAway(error);
    setter(value);
  }

  function goTo(next: number) {
    setPutAway(error);
    setStep(next);
  }

  function enter() {
    if (pending || stepProblem[1] !== null || stepProblem[2] !== null || stepProblem[3] !== null) return;
    onEnter({
      category_id: categoryId,
      cost_center_id: costCenterId || undefined,
      payment_term_id: hasInstallments ? undefined : termId || undefined,
      received_on: receivedOn || undefined,
      products: chosen,
    });
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (step === STEPS.length - 1) enter();
    else if (stepProblem[step] === null) goTo(step + 1);
  }

  return (
    <form className="record-form" onSubmit={submit}>
      <Stepper steps={STEPS.map((name) => t(name))} current={step} />
      {step === 1 && (
        <section className="form-section" aria-labelledby="invoice-info">
          <h3 id="invoice-info">{t("What the invoice says")}</h3>
          <p>
            <strong>{preview.issuer.name}</strong> · {preview.issuer.document}
          </p>
          <p>
            {rich("NF-e no. {number}, series {series} · issued on {date} · total {amount}", {
              number: preview.number,
              series: preview.series,
              date: formatDate(preview.issued_on),
              amount: <strong>{formatMoney(preview.total)}</strong>,
            })}
          </p>
          <p className="muted tabular">{t("Key: {key}", { key: preview.access_key })}</p>
          {unregistered && (
            <Notice
              tone="warning"
              message={t("Supplier not registered: register the supplier with the document {document} before entering the invoice.", {
                document: preview.issuer.document,
              })}
            />
          )}
          {preview.entered !== null && (
            <>
              <Notice
                tone="warning"
                message={t("This invoice has already been entered as purchase no. {number}.", { number: preview.entered.number })}
              />
              <p>
                <Link to={`/companies/${companyId}/purchases`} onClick={onCancel}>
                  {t("See in purchases")}
                </Link>
              </p>
            </>
          )}
          {unfinished && (
            <p className="muted" role="status">
              {t("The entry of this invoice was left incomplete (purchase no. {number}): entering it again finishes what was missing.", {
                number: preview.entered?.number ?? "",
              })}
            </p>
          )}
          {hasInstallments ? (
            <ul className="history" aria-label={t("Invoice installments")}>
              {preview.installments.map((installment, index) => (
                <li key={index}>
                  {formatDate(installment.due_date)} · {formatMoney(installment.amount)}
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted">{t("The invoice gives no installments.")}</p>
          )}
        </section>
      )}
      {step === 2 && (
        <section className="form-section" aria-labelledby="invoice-where">
          <h3 id="invoice-where">{t("Nature of the purchase")}</h3>
          <div className="field-grid">
            <label>
              {t("Category of the payables")}
              <select value={categoryId} onChange={(event) => change(setCategoryId, event.target.value)}>
                <option value="" disabled>
                  {t("Choose a category")}
                </option>
                {categoryRows.map(({ category: row, depth }) => (
                  <option key={row.id} value={row.id}>
                    {"— ".repeat(depth)}
                    {row.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("Cost center")}
              <select value={costCenterId} onChange={(event) => change(setCostCenterId, event.target.value)}>
                <option value="">{t("None")}</option>
                {targets.costCenters
                  .filter((center) => center.active)
                  .map((center) => (
                    <option key={center.id} value={center.id}>
                      {center.name}
                    </option>
                  ))}
              </select>
            </label>
            {!hasInstallments && (
              <label>
                {t("Payment term")}
                <select value={termId} onChange={(event) => change(setTermId, event.target.value)}>
                  <option value="">{t("Cash, on the day of receipt")}</option>
                  {targets.terms
                    .filter((item) => item.active)
                    .map((item) => (
                      <option key={item.id} value={item.id}>
                        {item.name}
                      </option>
                    ))}
                </select>
              </label>
            )}
            <label>
              {t("Day the goods arrived")}
              <input type="date" value={receivedOn} onChange={(event) => change(setReceivedOn, event.target.value)} />
            </label>
          </div>
        </section>
      )}
      {step === 3 && (
        <section className="form-section" aria-labelledby="invoice-items">
          <h3 id="invoice-items">{t("NF-e products")}</h3>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Code")}</th>
                  <th>{t("Description")}</th>
                  <th className="num">{t("Quantity")}</th>
                  <th className="num">{t("Cost")}</th>
                  <th className="num">IPI</th>
                  <th>{t("Product")}</th>
                </tr>
              </thead>
              <tbody>
                {preview.items.map((item, index) => (
                  <tr key={item.number}>
                    <td>{item.code}</td>
                    <td>{item.description}</td>
                    <td className="num">
                      {formatDecimal(item.quantity)} {item.unit}
                    </td>
                    <td className="num">{formatMoney(item.cost)}</td>
                    <td className="num">{formatMoney(item.ipi)}</td>
                    <td>
                      <select
                        aria-label={t("Product of item {number}", { number: item.number })}
                        value={chosen[index]}
                        onChange={(event) => change(setChosen, chosen.map((id, place) => (place === index ? event.target.value : id)))}
                      >
                        <option value="" disabled>
                          {t("Choose the product")}
                        </option>
                        {products.map((product) => (
                          <option key={product.id} value={product.id}>
                            {product.name}
                            {product.sku !== null && ` (${product.sku})`}
                          </option>
                        ))}
                      </select>
                      {chosen[index] === "" && <span className="badge badge-dot badge-warning">{t("No product")}</span>}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {withoutProduct > 0 && (
            <p className="muted">
              {withoutProduct === 1
                ? t("One item has no product: the code on the invoice is not the code of any product. Choose the product of each one.")
                : t("{count} items have no product: the code on the invoice is not the code of any product. Choose the product of each one.", {
                    count: withoutProduct,
                  })}
            </p>
          )}
        </section>
      )}
      {step === 4 && (
        <section className="form-section" aria-labelledby="invoice-done">
          <h3 id="invoice-done">{t("Finish the entry")}</h3>
          <dl className="summary">
            <dt>{t("Supplier")}</dt>
            <dd>{preview.issuer.name}</dd>
            <dt>{t("Invoice")}</dt>
            <dd>{t("no. {number}, series {series} · {total}", { number: preview.number, series: preview.series, total: formatMoney(preview.total) })}</dd>
            <dt>{t("Items")}</dt>
            <dd>{preview.items.length}</dd>
            <dt>{t("Payables")}</dt>
            <dd>
              {hasInstallments
                ? preview.installments.length === 1
                  ? t("{count} installment, the invoice's", { count: preview.installments.length })
                  : t("{count} installments, the invoice's", { count: preview.installments.length })
                : (term?.name ?? t("Cash, on the day of receipt"))}
            </dd>
            <dt>{t("Category")}</dt>
            <dd>{category?.name}</dd>
            <dt>{t("Arrived on")}</dt>
            <dd>{formatDate(receivedOn)}</dd>
          </dl>
          <p className="muted">{t("On entering, the purchase is created as received: the payables are entered and the stock goes up.")}</p>
        </section>
      )}
      {stepProblem[step] !== null && stepProblem[step] !== undefined && step !== 1 && <p className="muted">{stepProblem[step]}</p>}
      <Notice message={step === STEPS.length - 1 && error !== putAway ? error : null} />
      <div className="form-footer">
        {step === 1 ? (
          <>
            <BackButton disabled={pending} onClick={onCancel} />
            <Button type="button" variant="ghost" disabled={pending} onClick={onChangeFile}>
              {t("Change file")}
            </Button>
          </>
        ) : (
          <Button type="button" variant="ghost" disabled={pending} onClick={() => goTo(step - 1)}>
            {t("Back")}
          </Button>
        )}
        {step < STEPS.length - 1 ? (
          <Button type="submit" disabled={stepProblem[step] !== null}>
            {t("Continue")}
          </Button>
        ) : (
          <Button type="submit" icon={CheckIcon} pending={pending}>
            {t("Enter purchase")}
          </Button>
        )}
      </div>
    </form>
  );
}
