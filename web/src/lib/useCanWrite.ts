import { useParams } from "react-router-dom";
import type { Area } from "../api";
import { useAuth } from "../auth";

/**
 * Whether the signed-in person may change an area of the company in the URL. The API refuses a write
 * from a role that does not reach it either way: this only keeps the page from offering what would be
 * refused. The permissions come from `/auth/me`, so the table of roles lives in one place (ADR 0026).
 */
export function useCanWrite(area: Area): boolean {
  const { companyId } = useParams();
  const { session } = useAuth();
  if (session.status !== "authenticated") {
    return false;
  }
  const company = session.profile.companies.find((candidate) => candidate.id === companyId);
  return company?.permissions[area] === "write";
}
