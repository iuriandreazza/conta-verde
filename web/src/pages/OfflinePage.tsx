import { useState } from "react";
import { useAuth } from "../auth";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { useI18n } from "../i18n";
import { isOfflineEnabled, setOfflineEnabled } from "../pwa/client";

/** Where the person turns the offline mode on or off for this device, knowing what that leaves on it. It is their device and their call. */
export function OfflinePage() {
  const { t, rich } = useI18n();
  const { session } = useAuth();
  const userId = session.status === "authenticated" ? session.profile.user.id : null;
  const [enabled, setEnabled] = useState(() => userId !== null && isOfflineEnabled(userId));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const supported = typeof navigator !== "undefined" && "serviceWorker" in navigator && typeof caches !== "undefined";

  async function change(on: boolean) {
    if (userId === null || busy) return;
    setBusy(true);
    setError(null);
    try {
      await setOfflineEnabled(on, userId);
      setEnabled(on);
    } catch {
      setError(t("Could not change the offline mode on this device."));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <PageHeader
        title={t("Offline use")}
        description={t("The app can be installed on the device and open without internet. With the offline mode on, the screens you opened (customers, products, entries, sales, quotes, stock, balances, the bank statement to reconcile and the dashboard) are kept here to look at when the connection drops.")}
      />
      <section className="card narrow">
        <h2>{t("On this device")}</h2>
        {!supported && <Notice message={t("This browser does not allow the offline mode.")} />}
        <p>{rich("Offline mode is {state}.", { state: <strong>{enabled ? t("turned on") : t("turned off")}</strong> })}</p>
        <ul>
          <li>
            {rich(
              "When on, a copy of what you look at is kept {device}, including customers' personal data (document, address, contacts), for at most {days} or until you {signOut}, whichever comes first. Use it only on a device of your own.",
              {
                device: <strong>{t("on this device")}</strong>,
                days: <strong>{t("7 days")}</strong>,
                signOut: <strong>{t("sign out")}</strong>,
              },
            )}
          </li>
          <li>{t("The terms you typed in the searches of these screens are also kept, together with the result.")}</li>
          <li>
            {rich("Offline you can only {view}: nothing is saved without a connection, and a screen that cannot reach the server says so.", {
              view: <strong>{t("view")}</strong>,
            })}
          </li>
          <li>{t("Never kept: members, API keys, webhooks, accountant, fiscal, attachments, exports and histories.")}</li>
          <li>{t("Turning it off deletes the copy at once.")}</li>
        </ul>
        <Notice message={error} />
        <Button type="button" pending={busy} disabled={!supported || userId === null} onClick={() => void change(!enabled)}>
          {enabled ? t("Turn off and delete the copy") : t("Turn on the offline mode on this device")}
        </Button>
      </section>
    </>
  );
}
