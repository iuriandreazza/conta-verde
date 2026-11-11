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
import { useI18n } from "../i18n";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function CollectionPage() {
  const { t, rich } = useI18n();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const targets = useEntryTargets(companyId);
  const [settings, setSettings] = useState<CollectionSettings | null>(null);
  const [provider, setProvider] = useState<string | null | undefined>(undefined);
  const [plan, setPlan] = useState<DunningPlan | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let current = true;
    Promise.all([api.getCollectionSettings(companyId), api.getCollection(companyId)])
      .then(([stored, collection]) => {
        if (current) {
          setSettings(stored);
          setProvider(collection.provider);
        }
      })
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    // The reminders are a section of their own: if they cannot be read, the settings above still can.
    api
      .getDunningPlan(companyId)
      .then((dunning) => current && setPlan(dunning))
      .catch((failure: unknown) => current && setPlanError(errorMessage(failure)));
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
      setError(t("Choose the account the money from the charges lands in."));
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
      setSaved(t("Collection settings saved."));
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  const ready = settings !== null && targets.loaded && provider !== undefined;
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
        title={t("Collection")}
        description={t("Where the money from the charges lands and how the provider's fee is recorded.")}
      />
      <Notice message={error ?? targets.error} />
      <Notice message={saved} tone="success" />
      {!ready && error === null && targets.error === null ? (
        <p role="status" className="status">
          <Spinner /> {t("Loading…")}
        </p>
      ) : (
        ready && (
          <section className="card">
            <h2>{t("Payment provider")}</h2>
            {provider === null ? (
              <p className="muted">
                {t("No provider is set up in this installation (PAYMENT_PROVIDER variable): charges cannot be issued.")}
              </p>
            ) : (
              <>
                <p>
                  {rich("Provider: {name}", { name: <strong>{provider}</strong> })}
                  {provider === "fake" && ` ${t("(test: the charges are fake and cannot be paid)")}`}
                </p>
                <p className="muted">
                  {t("Give this address in the provider's panel so that it tells us when a charge is paid:")}
                </p>
                <p>
                  <code className="tabular">{webhook}</code>
                </p>
              </>
            )}
            <form onSubmit={(event) => void handleSubmit(event)} className="settle-form">
              <h2>{t("Receiving payments")}</h2>
              <label>
                {t("Receiving account")}
                <select
                  name="bank_account_id"
                  required
                  disabled={!canWrite}
                  defaultValue={settings.bank_account_id ?? ""}
                >
                  <option value="" disabled>
                    {t("Choose an account")}
                  </option>
                  {banks.map((bank) => (
                    <option key={bank.id} value={bank.id}>
                      {bank.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                {t("Provider fee category")}
                <select name="fee_category_id" disabled={!canWrite} defaultValue={settings.fee_category_id ?? ""}>
                  <option value="">{t("Do not record the fee (it stays only on the charge)")}</option>
                  {categories.map((category) => (
                    <option key={category.id} value={category.id}>
                      {category.name}
                    </option>
                  ))}
                </select>
              </label>
              {canWrite && (
                <Button type="submit" icon={CheckIcon} pending={saving}>
                  {t("Save")}
                </Button>
              )}
            </form>
          </section>
        )
      )}
      <Notice message={planError} />
      {ready && plan !== null && (
        <section className="card">
          <DunningPlanForm companyId={companyId} plan={plan} canWrite={canWrite} onSaved={setPlan} />
        </section>
      )}
    </>
  );
}
