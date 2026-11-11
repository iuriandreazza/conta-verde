import { useT } from "../i18n";
import { useOfflineStatus } from "../pwa/status";

const time = new Intl.DateTimeFormat("pt-BR", { dateStyle: "short", timeStyle: "short" });

/** Said whenever the screen may be showing the device's copy: what the person sees is old, and nothing can be saved until the network is back. */
export function OfflineBanner() {
  const t = useT();
  const { offline, cachedAt, stale } = useOfflineStatus();
  if (!offline && !stale) return null;
  return (
    <div className="banner" role="status">
      <span>
        {offline ? t("No connection.") : t("The connection is back.")}{" "}
        {cachedAt !== null ? t("What shows is partly a copy kept on this device, from {time}.", { time: time.format(cachedAt) }) : t("The data shown may be out of date.")}{" "}
        {offline ? t("Changes cannot be saved while you are offline.") : t("Refresh to see the data as it is now.")}
      </span>
      {!offline && (
        <button type="button" className="btn btn-ghost btn-small" onClick={() => window.location.reload()}>
          {t("Refresh")}
        </button>
      )}
    </div>
  );
}
