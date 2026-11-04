import { Link, Navigate } from "react-router-dom";
import { useAuth } from "../auth";

export function CompaniesPage() {
  const { session } = useAuth();
  if (session.status !== "authenticated") {
    return null;
  }
  const { companies } = session.profile;

  // The common case is a single company: skip the picker.
  if (companies.length === 1) {
    return <Navigate to={`/companies/${companies[0].id}/parties`} replace />;
  }

  return (
    <section className="card narrow">
      <h1>Choose a company</h1>
      <ul className="plain-list">
        {companies.map((company) => (
          <li key={company.id}>
            <Link to={`/companies/${company.id}/parties`}>{company.name}</Link>
          </li>
        ))}
      </ul>
    </section>
  );
}
