import { Link } from "react-router-dom";
import { useT } from "../i18n";

/** What an address the app has no page for shows, so a mistyped or mangled link is never a blank screen. */
export function NotFoundPage() {
  const t = useT();
  return (
    <section className="card narrow">
      <h1>{t("Page not found")}</h1>
      <p>{t("This address does not lead to any page. Check the link you were given.")}</p>
      <p>
        <Link to="/">{t("Back to the start")}</Link>
      </p>
    </section>
  );
}
