import { useState, type DragEvent, type FormEvent } from "react";
import { Link } from "react-router-dom";
import type { Category, CostCenter, InvoiceEntryParams, InvoicePreview, PaymentTerm, Product } from "../api";
import { treeRows } from "../lib/categoryTree";
import { formatDecimal, formatMoney } from "../lib/decimal";
import { formatDate } from "../lib/entryInstallments";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { Notice } from "./Notice";
import { useSheetDirty } from "./Sheet";

/** The most the API reads of an invoice: a bigger file is refused before it is sent. */
export const MAX_INVOICE_BYTES = 1024 * 1024;

/** Why the file cannot be an invoice, or `null` when the server may decide. */
export function invoiceFileProblem(file: File): string | null {
  if (!file.name.toLowerCase().endsWith(".xml")) return "Escolha o arquivo XML da nota (.xml).";
  if (file.size > MAX_INVOICE_BYTES) return "O arquivo tem mais de 1 MiB: uma NF-e não é tão grande. Confira se é o XML certo.";
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

/** Enters a supplier's NF-e as a purchase: step one chooses the XML, step two shows what it says and asks where it goes. */
export function PurchaseImportForm(props: PurchaseImportFormProps) {
  return props.preview === null ? <ChooseFile {...props} /> : <EnterInvoice {...props} preview={props.preview} />;
}

function ChooseFile({ pending, error, onRead, onCancel }: PurchaseImportFormProps) {
  const [file, setFile] = useState<File | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  // Something is being dragged over the box: it says where to let go.
  const [dragging, setDragging] = useState(false);

  function choose(chosen: File | null) {
    setFile(chosen);
    setProblem(chosen === null ? null : invoiceFileProblem(chosen));
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
      <section className="form-section" aria-labelledby="import-file">
        <h3 id="import-file">Arquivo da nota</h3>
        <p className="muted">
          A NF-e fica salva no seu computador como um arquivo XML. Ao ler o arquivo, você vê a nota, o fornecedor e os produtos antes de lançar a compra.
        </p>
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
            Escolha um arquivo
            <input
              type="file"
              aria-label="Arquivo XML da NF-e"
              accept=".xml,application/xml,text/xml"
              disabled={pending}
              onChange={(event) => choose(event.target.files?.[0] ?? null)}
            />
          </label>
          <p className="muted">Ou arraste o XML para este espaço</p>
          {file !== null && <p className="dropzone-file">{file.name}</p>}
        </div>
      </section>
      <Notice message={problem ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" pending={pending} disabled={file === null || problem !== null}>
          Ler arquivo
        </Button>
      </div>
    </form>
  );
}

function EnterInvoice({ companyId, targets, preview, pending, error, onEnter, onChangeFile, onCancel }: PurchaseImportFormProps & { preview: InvoicePreview }) {
  const [chosen, setChosen] = useState<string[]>(() => preview.items.map((item) => item.product_id ?? ""));
  const [fieldError, setFieldError] = useState<string | null>(null);
  const markDirty = useSheetDirty();
  const categoryRows = treeRows(targets.categories).filter(({ category }) => category.active && category.dre_group !== "transfers");
  const products = targets.products.filter((product) => product.active || chosen.includes(product.id));
  const withoutProduct = chosen.filter((id) => id === "").length;
  const unregistered = preview.supplier_id === null;
  const alreadyEntered = preview.entered?.status === "received";
  const unfinished = preview.entered !== null && !alreadyEntered;
  const blocked = unregistered || alreadyEntered || withoutProduct > 0;

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending || blocked) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (field("category_id") === "") {
      setFieldError("Escolha a categoria das contas a pagar.");
      return;
    }
    setFieldError(null);
    onEnter({
      category_id: field("category_id"),
      cost_center_id: field("cost_center_id") || undefined,
      payment_term_id: preview.installments.length === 0 ? field("payment_term_id") || undefined : undefined,
      received_on: field("received_on") || undefined,
      products: chosen,
    });
  }

  return (
    <form className="record-form" onSubmit={submit}>
      <section className="form-section" aria-labelledby="invoice-info">
        <h3 id="invoice-info">O que a nota diz</h3>
        <p>
          <strong>{preview.issuer.name}</strong> · {preview.issuer.document}
        </p>
        <p>
          NF-e nº {preview.number}, série {preview.series} · emitida em {formatDate(preview.issued_on)} · total{" "}
          <strong>{formatMoney(preview.total)}</strong>
        </p>
        <p className="muted tabular">Chave: {preview.access_key}</p>
        {unregistered && (
          <Notice
            tone="warning"
            message={`Fornecedor não cadastrado: cadastre o fornecedor com o documento ${preview.issuer.document} antes de entrar a nota.`}
          />
        )}
        {preview.entered !== null && (
          <>
            <Notice tone="warning" message={`Esta nota já foi entrada como a compra nº ${preview.entered.number}.`} />
            <p>
              <Link to={`/companies/${companyId}/purchases`} onClick={onCancel}>
                Ver nas compras
              </Link>
            </p>
          </>
        )}
        {preview.installments.length === 0 ? (
          <p className="muted">A nota não informa parcelas.</p>
        ) : (
          <ul className="history" aria-label="Parcelas da nota">
            {preview.installments.map((installment, index) => (
              <li key={index}>
                {formatDate(installment.due_date)} · {formatMoney(installment.amount)}
              </li>
            ))}
          </ul>
        )}
      </section>
      <section className="form-section" aria-labelledby="invoice-items">
        <h3 id="invoice-items">Itens</h3>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Código</th>
                <th>Descrição</th>
                <th className="num">Quantidade</th>
                <th className="num">Custo</th>
                <th className="num">IPI</th>
                <th>Produto</th>
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
                      aria-label={`Produto do item ${item.number}`}
                      value={chosen[index]}
                      disabled={alreadyEntered}
                      onChange={(event) => {
                        markDirty();
                        setChosen(chosen.map((id, place) => (place === index ? event.target.value : id)));
                      }}
                    >
                      <option value="" disabled>
                        Escolha o produto
                      </option>
                      {products.map((product) => (
                        <option key={product.id} value={product.id}>
                          {product.name}
                          {product.sku !== null && ` (${product.sku})`}
                        </option>
                      ))}
                    </select>
                    {chosen[index] === "" && <span className="badge badge-dot badge-warning">Sem produto</span>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {unfinished && (
          <p className="muted" role="status">
            A entrada desta nota ficou incompleta (compra nº {preview.entered?.number}): entrar de novo conclui o que faltou.
          </p>
        )}
        {withoutProduct > 0 && !alreadyEntered && (
          <p className="muted">
            {withoutProduct === 1 ? "Um item está sem produto" : `${withoutProduct} itens estão sem produto`}: o código da nota não é o código de nenhum produto.
            Escolha o produto de cada um.
          </p>
        )}
      </section>
      <section className="form-section" aria-labelledby="invoice-where">
        <h3 id="invoice-where">Onde lançar</h3>
        <div className="field-grid">
          <label>
            Categoria das contas a pagar
            <select name="category_id" required defaultValue="" disabled={alreadyEntered}>
              <option value="" disabled>
                Escolha uma categoria
              </option>
              {categoryRows.map(({ category, depth }) => (
                <option key={category.id} value={category.id}>
                  {"— ".repeat(depth)}
                  {category.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Centro de custo
            <select name="cost_center_id" defaultValue="" disabled={alreadyEntered}>
              <option value="">Nenhum</option>
              {targets.costCenters
                .filter((center) => center.active)
                .map((center) => (
                  <option key={center.id} value={center.id}>
                    {center.name}
                  </option>
                ))}
            </select>
          </label>
          {preview.installments.length === 0 && (
            <label>
              Prazo de pagamento
              <select name="payment_term_id" defaultValue="" disabled={alreadyEntered}>
                <option value="">À vista, no dia do recebimento</option>
                {targets.terms
                  .filter((term) => term.active)
                  .map((term) => (
                    <option key={term.id} value={term.id}>
                      {term.name}
                    </option>
                  ))}
              </select>
            </label>
          )}
          <label>
            Dia em que a mercadoria chegou
            <input name="received_on" type="date" required defaultValue={preview.issued_on} disabled={alreadyEntered} />
          </label>
        </div>
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="button" variant="ghost" disabled={pending} onClick={onChangeFile}>
          Trocar arquivo
        </Button>
        <Button type="submit" icon={CheckIcon} pending={pending} disabled={blocked}>
          Entrar compra
        </Button>
      </div>
    </form>
  );
}
