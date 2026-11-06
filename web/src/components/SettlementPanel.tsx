import { useCallback, useEffect, useState, type FormEvent } from "react";
import {
  api,
  errorMessage,
  type BankAccount,
  type Entry,
  type PaymentMethod,
  type Settlement,
  type SettlementInput,
} from "../api";
import { formatMoney, isAmbiguousDecimal, toInputDecimal } from "../lib/decimal";
import { centsToApi, formatDate, isIsoDate, toCents } from "../lib/entryInstallments";
import {
  STATUS_LABELS,
  balance,
  balanceCents,
  installmentStatus,
  todayIso,
  type InstallmentStatus,
} from "../lib/settlement";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { Notice } from "./Notice";
import { Spinner } from "./Spinner";

const BADGES: Record<InstallmentStatus, string> = {
  open: "badge badge-dot badge-muted",
  partial: "badge badge-dot badge-warning",
  settled: "badge badge-dot badge-ok",
  overdue: "badge badge-dot badge-critical",
};

interface SettlementPanelProps {
  companyId: string;
  entry: Entry;
  banks: BankAccount[];
  methods: PaymentMethod[];
  canWrite: boolean;
  /** Called after a settlement or a reversal, so the list shows the new balances. */
  onChanged: () => void;
}

/**
 * The installments of an entry with what is left of each, the settlements made, and the form to settle
 * one: what is applied, with interest, fine and discount, from a bank account (ADR 0030). Whether an
 * installment is late is told here, with today's date where the person is.
 */
export function SettlementPanel({
  companyId,
  entry,
  banks,
  methods,
  canWrite,
  onChanged,
}: SettlementPanelProps) {
  const [settlements, setSettlements] = useState<Settlement[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [reversingId, setReversingId] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const today = todayIso();
  const open = entry.installments.filter((installment) => balanceCents(installment) > 0n);
  const [chosenNumber, setChosenNumber] = useState<number | null>(null);
  // What was chosen counts only while it still has something left: after it is settled, or another is
  // reopened by a reversal, the select shows the first that has, and that is what is sent.
  const installment = open.some((item) => item.number === chosenNumber)
    ? (chosenNumber as number)
    : (open[0]?.number ?? 1);
  // Bumped after a settlement so the form starts over.
  const [formVersion, setFormVersion] = useState(0);

  const load = useCallback(async () => {
    try {
      setSettlements(await api.listSettlements(companyId, entry.id));
      setError(null);
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }, [companyId, entry.id]);

  useEffect(() => {
    void load();
  }, [load]);

  const chosen = entry.installments.find((item) => item.number === installment);
  const activeBanks = banks.filter((bank) => bank.active);
  const activeMethods = methods.filter((method) => method.active);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    const money = (name: string, label: string): string | undefined | null => {
      const typed = field(name);
      if (typed === "") return undefined;
      // `1.000` could be a thousand or one: the person says which, as in every amount field.
      const cents = isAmbiguousDecimal(typed) ? null : toCents(typed);
      if (cents === null) {
        setFormError(`${label} precisa estar em reais e centavos, por exemplo 1.234,50.`);
        return null;
      }
      return centsToApi(cents);
    };
    const applied = money("applied", "O valor baixado");
    const interest = money("interest", "Os juros");
    const fine = money("fine", "A multa");
    const discount = money("discount", "O desconto");
    if (applied === undefined) {
      setFormError("Informe o valor baixado.");
      return;
    }
    if (applied === null || interest === null || fine === null || discount === null) {
      return;
    }
    if (!isIsoDate(field("settled_on"))) {
      setFormError("Informe a data da baixa.");
      return;
    }
    const input: SettlementInput = {
      installment,
      settled_on: field("settled_on"),
      applied,
      interest,
      fine,
      discount,
      bank_account_id: field("bank_account_id"),
      payment_method_id: field("payment_method_id") || undefined,
    };
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const made = await api.settleInstallment(companyId, entry.id, input);
      setSaved(`Baixa de ${formatMoney(made.cash)} registrada.`);
      setFormVersion((version) => version + 1);
      await load();
      onChanged();
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  async function reverse(settlement: Settlement) {
    if (reversingId !== null) {
      return;
    }
    setReversingId(settlement.id);
    setError(null);
    setSaved(null);
    try {
      await api.reverseSettlement(companyId, entry.id, settlement.id);
      await load();
      onChanged();
    } catch (failure) {
      // Another tab may have reversed it already: read what is true now, then say what happened
      // (the read clears the message).
      await load();
      setError(errorMessage(failure));
    } finally {
      setReversingId(null);
    }
  }

  return (
    <div className="settlements">
      <table>
        <thead>
          <tr>
            <th>Parcela</th>
            <th>Vencimento</th>
            <th className="num">Valor</th>
            <th className="num">Baixado</th>
            <th className="num">Saldo</th>
            <th>Situação</th>
          </tr>
        </thead>
        <tbody>
          {entry.installments.map((item) => {
            const status = installmentStatus(item, today);
            return (
              <tr key={item.number}>
                <td>{item.number}</td>
                <td className="tabular">{formatDate(item.due_date)}</td>
                <td className="num">{formatMoney(item.amount)}</td>
                <td className="num">{formatMoney(item.settled)}</td>
                <td className="num">{formatMoney(balance(item))}</td>
                <td>
                  <span className={BADGES[status]}>{STATUS_LABELS[status]}</span>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>

      <Notice message={error} />
      {settlements === null && error === null ? (
        <p role="status" className="status">
          <Spinner /> Carregando…
        </p>
      ) : (
        settlements !== null &&
        settlements.length > 0 && (
          <ul className="history">
            {settlements.map((settlement) => (
              <li key={settlement.id} className={settlement.reversed ? "muted" : undefined}>
                Parcela {settlement.installment}, {formatDate(settlement.settled_on)}:{" "}
                {formatMoney(settlement.applied)} baixado
                {settlement.interest !== "0" && `, juros ${formatMoney(settlement.interest)}`}
                {settlement.fine !== "0" && `, multa ${formatMoney(settlement.fine)}`}
                {settlement.discount !== "0" && `, desconto ${formatMoney(settlement.discount)}`}; saiu{" "}
                {formatMoney(settlement.cash)} da conta{" "}
                {banks.find((bank) => bank.id === settlement.bank_account_id)?.name ?? ""}
                {settlement.reversed ? (
                  <span className="badge badge-muted"> Estornada</span>
                ) : (
                  canWrite && (
                    <Button
                      type="button"
                      variant="ghost"
                      className="btn-small"
                      aria-label={`Estornar a baixa de ${formatMoney(settlement.applied)} da parcela ${settlement.installment}`}
                      pending={reversingId === settlement.id}
                      disabled={reversingId !== null}
                      onClick={() => void reverse(settlement)}
                    >
                      Estornar
                    </Button>
                  )
                )}
              </li>
            ))}
          </ul>
        )
      )}

      {canWrite &&
        (open.length === 0 ? (
          <p className="muted">Todas as parcelas estão pagas.</p>
        ) : (
          <form onSubmit={handleSubmit} key={`${installment}-${formVersion}`} className="settle-form">
            <h3>Baixar uma parcela</h3>
            <label>
              Parcela
              <select
                name="installment"
                value={installment}
                onChange={(event) => setChosenNumber(Number(event.target.value))}
              >
                {open.map((item) => (
                  <option key={item.number} value={item.number}>
                    {item.number} — vence em {formatDate(item.due_date)}, saldo {formatMoney(balance(item))}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Data da baixa
              <input name="settled_on" type="date" required defaultValue={today} />
            </label>
            <label>
              Valor baixado
              <input
                name="applied"
                inputMode="decimal"
                autoComplete="off"
                defaultValue={chosen ? toInputDecimal(balance(chosen)) : ""}
              />
            </label>
            <label>
              Juros
              <input name="interest" inputMode="decimal" autoComplete="off" placeholder="0,00" />
            </label>
            <label>
              Multa
              <input name="fine" inputMode="decimal" autoComplete="off" placeholder="0,00" />
            </label>
            <label>
              Desconto
              <input name="discount" inputMode="decimal" autoComplete="off" placeholder="0,00" />
            </label>
            <label>
              Conta
              <select name="bank_account_id" required defaultValue="">
                <option value="" disabled>
                  Escolha uma conta
                </option>
                {activeBanks.map((bank) => (
                  <option key={bank.id} value={bank.id}>
                    {bank.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Forma de pagamento
              <select name="payment_method_id" defaultValue="">
                <option value="">Nenhuma</option>
                {activeMethods.map((method) => (
                  <option key={method.id} value={method.id}>
                    {method.name}
                  </option>
                ))}
              </select>
            </label>
            <Notice message={formError} />
            <Notice message={saved} tone="success" />
            <Button type="submit" icon={CheckIcon} pending={saving}>
              Baixar
            </Button>
          </form>
        ))}
    </div>
  );
}
