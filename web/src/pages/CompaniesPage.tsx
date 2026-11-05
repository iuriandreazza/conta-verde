import { motion } from "motion/react";
import { Link, Navigate } from "react-router-dom";
import { useAuth } from "../auth";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";

export function CompaniesPage() {
  const { session } = useAuth();
  if (session.status !== "authenticated") {
    return null;
  }
  const { companies } = session.profile;

  // The common case is a single company: skip the picker.
  if (companies.length === 1) {
    return <Navigate to={`/companies/${companies[0].id}`} replace />;
  }

  return (
    <section className="narrow-wide">
      <h1>Escolha uma empresa</h1>
      <ul className="plain-list company-list">
        {companies.map((company, index) => (
          <motion.li
            key={company.id}
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: index * 0.05, type: "spring", stiffness: 300, damping: 26 }}
            whileHover={{ y: -2 }}
            whileTap={{ scale: 0.98 }}
          >
            <Link to={`/companies/${company.id}`} className="company-card">
              <BriefcaseBusinessIcon size={22} aria-hidden />
              <span>{company.name}</span>
            </Link>
          </motion.li>
        ))}
      </ul>
    </section>
  );
}
