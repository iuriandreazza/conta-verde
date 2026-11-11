import { Navigate, Outlet, useLocation } from "react-router-dom";
import { useAuth } from "../auth";
import { useT } from "../i18n";

export function RequireAuth() {
  const t = useT();
  const { session } = useAuth();
  const { pathname, search } = useLocation();
  if (session.status === "loading") {
    return <p role="status">{t("Loading…")}</p>;
  }
  // Where the person was going comes back after signing in: the link of an e-mail leads to a page of the company.
  const login = `/login?next=${encodeURIComponent(pathname + search)}`;
  return session.status === "authenticated" ? <Outlet /> : <Navigate to={login} replace />;
}
