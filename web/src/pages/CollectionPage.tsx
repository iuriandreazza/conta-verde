import { useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type CollectionSettings, type DunningPlan } from "../api";
import { DunningPlanForm } from "../components/DunningPlanForm";
import { Button } from "../components/Button";
import { CheckIcon } from "../components/icons/check";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { useCanWrite } from "../lib/useCanWrite";
import { useEntryTargets } from "../lib/useEntryTargets";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function CollectionPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const targets = useEntryTargets(companyId);
  const [settings, setSettings] = useState<CollectionSettings | null>(null);
  const [provider, setProvider] = useState<string | null | undefined>(undefined);
  const [plan, setPlan] = useState<DunningPlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let current = true;
    Promise.all([
      api.getCollectionSettings(companyId),
      api.getCollection(companyId),
      api.getDunningPlan(companyId),
    ])
      .then(([stored, collection, dunning]) => {
        if (current) {
          setSettings(stored);
          setProvider(collection.provider);
          setPlan(dunning);
        }
      })
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const bank = String(form.get("bank_account_id") ?? "");
    const category = String(form.get("fee_category_id") ?? "");
    if (bank === "") {
      setError("Escolha a conta em que o dinheiro das cobranças cai.");
      return;
    }
    setSaving(true);
    setError(null);
    setSaved(null);
    try {
      setSettings(
        await api.replaceCollectionSettings(companyId, {
          bank_account_id: bank,
          fee_category_id: category === "" ? null : category,
        }),
      );
      setSaved("Configuração da cobrança salva.");
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  const ready = settings !== null && targets.loaded && provider !== undefined && plan !== null;
  const banks = targets.banks.filter((bank) => bank.active || bank.id === settings?.bank_account_id);
  const categories = targets.categories.filter(
    (category) =>
      (category.active && category.dre_group !== "transfers") || category.id === settings?.fee_category_id,
  );
  const webhook =
    provider === null || provider === undefined
      ? null
      : `${window.location.origin}/api/public/payments/${companyId}/webhooks/${provider}`;

  return (
    <>
      <PageHeader
        title="Cobrança"
        description="Onde o dinheiro das cobranças cai e como a taxa do provedor é lançada."
      />
      <Notice message={error ?? targets.error} />
      <Notice message={saved} tone="success" />
      {!ready && error === null && targets.error === null ? (
        <p role="status" className="status">
          <Spinner /> Carregando…
        </p>
      ) : (
        ready && (
          <section className="card">
            <h2>Provedor de pagamentos</h2>
            {provider === null ? (
              <p className="muted">
                Nenhum provedor está configurado nesta instalação (variável PAYMENT_PROVIDER): não é possível
                emitir cobranças.
              </p>
            ) : (
              <>
                <p>
                  Provedor: <strong>{provider}</strong>
                  {provider === "fake" && " (de teste: as cobranças são de mentira e não podem ser pagas)"}
                </p>
                <p className="muted">
                  Informe este endereço no painel do provedor para ele avisar quando uma cobrança for paga:
                </p>
                <p>
                  <code className="tabular">{webhook}</code>
                </p>
              </>
            )}
            <form onSubmit={(event) => void handleSubmit(event)} className="settle-form">
              <h2>Recebimento</h2>
              <label>
                Conta de recebimento
                <select
                  name="bank_account_id"
                  required
                  disabled={!canWrite}
                  defaultValue={settings.bank_account_id ?? ""}
                >
                  <option value="" disabled>
                    Escolha uma conta
                  </option>
                  {banks.map((bank) => (
                    <option key={bank.id} value={bank.id}>
                      {bank.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Categoria da taxa do provedor
                <select name="fee_category_id" disabled={!canWrite} defaultValue={settings.fee_category_id ?? ""}>
                  <option value="">Não lançar a taxa (fica só na cobrança)</option>
                  {categories.map((category) => (
                    <option key={category.id} value={category.id}>
                      {category.name}
                    </option>
                  ))}
                </select>
              </label>
              {canWrite && (
                <Button type="submit" icon={CheckIcon} pending={saving}>
                  Salvar
                </Button>
              )}
            </form>
          </section>
        )
      )}
      {ready && (
        <section className="card">
          <DunningPlanForm companyId={companyId} plan={plan} canWrite={canWrite} onSaved={setPlan} />
        </section>
      )}
    </>
  );
}
