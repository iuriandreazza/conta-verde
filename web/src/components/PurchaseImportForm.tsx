import { useEffect, useState, type DragEvent, type FormEvent } from "react";
import { Link } from "react-router-dom";
import type { Category, CostCenter, InvoiceEntryParams, InvoicePreview, PaymentTerm, Product } from "../api";
import { treeRows } from "../lib/categoryTree";
import { formatDecimal, formatMoney } from "../lib/decimal";
import { formatDate, wholeDate } from "../lib/entryInstallments";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { Notice } from "./Notice";
import { Stepper } from "./Stepper";
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

/**
 * Enters a supplier's NF-e as a purchase in five steps ([ADR 0057]): the file, what the invoice says, the nature of the
 * purchase (where the payables go), the products of the items, and a last look before it is entered.
 */
const STEPS = ["Arquivo XML", "Dados da nota", "Natureza da compra", "Produtos", "Concluir"];

/** Enters a supplier's NF-e as a purchase, step by step. */
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
      <Stepper steps={STEPS} current={0} />
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
    1: unregistered ? "Cadastre o fornecedor para continuar." : alreadyEntered ? "Esta nota já foi entrada." : null,
    2: categoryId === "" ? "Escolha a categoria das contas a pagar." : wholeDate(receivedOn) === "" ? "Informe o dia em que a mercadoria chegou." : null,
    3: withoutProduct > 0 ? "Escolha o produto de cada item." : null,
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
      <Stepper steps={STEPS} current={step} />
      {step === 1 && (
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
          {unfinished && (
            <p className="muted" role="status">
              A entrada desta nota ficou incompleta (compra nº {preview.entered?.number}): entrar de novo conclui o que faltou.
            </p>
          )}
          {hasInstallments ? (
            <ul className="history" aria-label="Parcelas da nota">
              {preview.installments.map((installment, index) => (
                <li key={index}>
                  {formatDate(installment.due_date)} · {formatMoney(installment.amount)}
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted">A nota não informa parcelas.</p>
          )}
        </section>
      )}
      {step === 2 && (
        <section className="form-section" aria-labelledby="invoice-where">
          <h3 id="invoice-where">Natureza da compra</h3>
          <div className="field-grid">
            <label>
              Categoria das contas a pagar
              <select value={categoryId} onChange={(event) => change(setCategoryId, event.target.value)}>
                <option value="" disabled>
                  Escolha uma categoria
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
              Centro de custo
              <select value={costCenterId} onChange={(event) => change(setCostCenterId, event.target.value)}>
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
            {!hasInstallments && (
              <label>
                Prazo de pagamento
                <select value={termId} onChange={(event) => change(setTermId, event.target.value)}>
                  <option value="">À vista, no dia do recebimento</option>
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
              Dia em que a mercadoria chegou
              <input type="date" value={receivedOn} onChange={(event) => change(setReceivedOn, event.target.value)} />
            </label>
          </div>
        </section>
      )}
      {step === 3 && (
        <section className="form-section" aria-labelledby="invoice-items">
          <h3 id="invoice-items">Produtos da NF-e</h3>
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
                        onChange={(event) => change(setChosen, chosen.map((id, place) => (place === index ? event.target.value : id)))}
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
          {withoutProduct > 0 && (
            <p className="muted">
              {withoutProduct === 1 ? "Um item está sem produto" : `${withoutProduct} itens estão sem produto`}: o código da nota não é o código de nenhum produto.
              Escolha o produto de cada um.
            </p>
          )}
        </section>
      )}
      {step === 4 && (
        <section className="form-section" aria-labelledby="invoice-done">
          <h3 id="invoice-done">Concluir a entrada</h3>
          <dl className="summary">
            <dt>Fornecedor</dt>
            <dd>{preview.issuer.name}</dd>
            <dt>Nota</dt>
            <dd>
              nº {preview.number}, série {preview.series} · {formatMoney(preview.total)}
            </dd>
            <dt>Itens</dt>
            <dd>{preview.items.length}</dd>
            <dt>Contas a pagar</dt>
            <dd>
              {hasInstallments
                ? `${preview.installments.length} ${preview.installments.length === 1 ? "parcela" : "parcelas"}, as da nota`
                : (term?.name ?? "À vista, no dia do recebimento")}
            </dd>
            <dt>Categoria</dt>
            <dd>{category?.name}</dd>
            <dt>Chegou em</dt>
            <dd>{formatDate(receivedOn)}</dd>
          </dl>
          <p className="muted">Ao entrar, a compra é criada como recebida: as contas a pagar são lançadas e o estoque aumenta.</p>
        </section>
      )}
      {stepProblem[step] !== null && stepProblem[step] !== undefined && step !== 1 && <p className="muted">{stepProblem[step]}</p>}
      <Notice message={step === STEPS.length - 1 && error !== putAway ? error : null} />
      <div className="form-footer">
        {step === 1 ? (
          <>
            <BackButton disabled={pending} onClick={onCancel} />
            <Button type="button" variant="ghost" disabled={pending} onClick={onChangeFile}>
              Trocar arquivo
            </Button>
          </>
        ) : (
          <Button type="button" variant="ghost" disabled={pending} onClick={() => goTo(step - 1)}>
            Voltar
          </Button>
        )}
        {step < STEPS.length - 1 ? (
          <Button type="submit" disabled={stepProblem[step] !== null}>
            Continuar
          </Button>
        ) : (
          <Button type="submit" icon={CheckIcon} pending={pending}>
            Entrar compra
          </Button>
        )}
      </div>
    </form>
  );
}
