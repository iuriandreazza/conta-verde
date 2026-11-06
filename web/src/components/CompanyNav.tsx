import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { Link, NavLink, useLocation } from "react-router-dom";
import type { AnimatedIcon } from "./Button";
import { BoxIcon } from "./icons/box";
import { BadgeAlertIcon } from "./icons/badge-alert";
import { CalendarDaysIcon } from "./icons/calendar-days";
import { ChartLineIcon } from "./icons/chart-line";
import { CheckIcon } from "./icons/check";
import { ChevronDownIcon } from "./icons/chevron-down";
import { CircleDollarSignIcon } from "./icons/circle-dollar-sign";
import { CreditCardIcon } from "./icons/credit-card";
import { FileTextIcon } from "./icons/file-text";
import { FolderTreeIcon } from "./icons/folder-tree";
import { HandCoinsIcon } from "./icons/hand-coins";
import { LayersIcon } from "./icons/layers";
import { RotateCCWIcon } from "./icons/rotate-ccw";
import { SendIcon } from "./icons/send";
import { UserPlusIcon } from "./icons/user-plus";
import { UsersIcon } from "./icons/users";
import { WalletIcon } from "./icons/wallet";
import { WavesIcon } from "./icons/waves";

interface NavItem {
  path: string;
  /** Which kind of entry the item shows: the receivables and the payables are the same page with `?tipo=`. */
  kind?: "receivable" | "payable";
  label: string;
  icon: AnimatedIcon;
}

interface NavSection {
  key: string;
  label: string;
  items: NavItem[];
}

function NavItemLink({ base, item }: { base: string; item: NavItem }) {
  const Icon = item.icon;
  const location = useLocation();
  if (item.path === "/entries") {
    // Three links to one page: the one that is current is told by the kind in the address.
    const to = `${base}${item.path}${item.kind ? `?tipo=${item.kind}` : ""}`;
    const current = location.pathname === `${base}${item.path}` && new URLSearchParams(location.search).get("tipo") === (item.kind ?? null);
    return (
      <Link to={to} aria-current={current ? "page" : undefined}>
        <Icon size={18} aria-hidden />
        {item.label}
      </Link>
    );
  }
  return (
    <NavLink to={`${base}${item.path}`} end={item.path === ""}>
      <Icon size={18} aria-hidden />
      {item.label}
    </NavLink>
  );
}

/** A group of sections the person can fold away; it starts open so nothing is hidden by default. */
function NavGroup({ base, section }: { base: string; section: NavSection }) {
  const [open, setOpen] = useState(true);
  return (
    <div className="nav-group">
      <button
        type="button"
        className="nav-group-toggle"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        {section.label}
        <motion.span className="nav-chevron" animate={{ rotate: open ? 0 : -90 }} transition={{ duration: 0.15 }}>
          <ChevronDownIcon size={14} aria-hidden />
        </motion.span>
      </button>
      <AnimatePresence initial={false}>
        {open && (
          <motion.div
            className="nav-group-items"
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.18, ease: "easeOut" }}
          >
            {section.items.map((item) => (
              <NavItemLink key={`${item.path}${item.kind ?? ""}`} base={base} item={item} />
            ))}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

const HOME: NavItem = { path: "", label: "Painel", icon: ChartLineIcon };

const SECTIONS: NavSection[] = [
  {
    key: "commercial",
    label: "Comercial",
    items: [
      { path: "/quotes", label: "Orçamentos", icon: FileTextIcon },
      { path: "/sales", label: "Vendas", icon: HandCoinsIcon },
      { path: "/contracts", label: "Contratos", icon: RotateCCWIcon },
      { path: "/commissions", label: "Comissões", icon: CircleDollarSignIcon },
      { path: "/price-lists", label: "Tabelas de preço", icon: LayersIcon },
    ],
  },
  {
    key: "finance",
    label: "Financeiro",
    items: [
      { path: "/entries", kind: "receivable", label: "Contas a receber", icon: HandCoinsIcon },
      { path: "/entries", kind: "payable", label: "Contas a pagar", icon: CreditCardIcon },
      { path: "/entries", label: "Lançamentos", icon: CircleDollarSignIcon },
      { path: "/cash-flow", label: "Fluxo de caixa", icon: WavesIcon },
      { path: "/transfers", label: "Transferências", icon: SendIcon },
      { path: "/reconciliation", label: "Conciliação", icon: CheckIcon },
      { path: "/recurrences", label: "Recorrências", icon: RotateCCWIcon },
      { path: "/collection", label: "Cobrança", icon: CreditCardIcon },
      { path: "/delinquency", label: "Inadimplência", icon: BadgeAlertIcon },
    ],
  },
  {
    key: "registers",
    label: "Cadastros",
    items: [
      { path: "/parties", label: "Clientes e fornecedores", icon: UsersIcon },
      { path: "/products", label: "Produtos e serviços", icon: BoxIcon },
      { path: "/categories", label: "Categorias", icon: FolderTreeIcon },
      { path: "/cost-centers", label: "Centros de custo", icon: LayersIcon },
      { path: "/bank-accounts", label: "Contas bancárias", icon: WalletIcon },
      { path: "/payment-methods", label: "Formas de pagamento", icon: CreditCardIcon },
      { path: "/payment-terms", label: "Prazos de pagamento", icon: CalendarDaysIcon },
    ],
  },
];

const REPORTS: NavItem = { path: "/reports", label: "Relatórios", icon: FileTextIcon };

const MEMBERS: NavSection = {
  key: "settings",
  label: "Configurações",
  items: [{ path: "/members", label: "Membros", icon: UserPlusIcon }],
};

/** The sections of a company, grouped; the link of the page being shown is marked as current. */
export function CompanyNav({
  companyId,
  canSeeMembers,
}: {
  companyId: string;
  /** The role reaches who belongs to the company: the others are not shown the link. */
  canSeeMembers: boolean;
}) {
  const base = `/companies/${companyId}`;
  return (
    <nav className="side-nav" aria-label="Seções da empresa">
      <NavItemLink base={base} item={HOME} />
      {SECTIONS.map((section) => (
        <NavGroup key={section.key} base={base} section={section} />
      ))}
      <NavItemLink base={base} item={REPORTS} />
      {canSeeMembers && <NavGroup base={base} section={MEMBERS} />}
    </nav>
  );
}
