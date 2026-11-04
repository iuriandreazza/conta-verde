import { Navigate, Outlet } from "react-router-dom";
import { useAuth } from "../auth";

export function RequireAuth() {
  const { session } = useAuth();
  if (session.status === "loading") {
    return <p role="status">Carregando…</p>;
  }
  return session.status === "authenticated" ? <Outlet /> : <Navigate to="/login" replace />;
}
