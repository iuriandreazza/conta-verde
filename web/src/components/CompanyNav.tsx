import { NavLink } from "react-router-dom";
import { BoxIcon } from "./icons/box";
import { CalendarDaysIcon } from "./icons/calendar-days";
import { CircleDollarSignIcon } from "./icons/circle-dollar-sign";
import { ChartLineIcon } from "./icons/chart-line";
import { CreditCardIcon } from "./icons/credit-card";
import { FolderTreeIcon } from "./icons/folder-tree";
import { LayersIcon } from "./icons/layers";
import { UserPlusIcon } from "./icons/user-plus";
import { UsersIcon } from "./icons/users";
import { WalletIcon } from "./icons/wallet";

/** The sections of a company; the link of the page being shown is marked as current. */
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
      <NavLink to={base} end>
        <ChartLineIcon size={18} aria-hidden />
        Painel
      </NavLink>
      <NavLink to={`${base}/entries`}>
        <CircleDollarSignIcon size={18} aria-hidden />
        Lançamentos
      </NavLink>
      <NavLink to={`${base}/parties`}>
        <UsersIcon size={18} aria-hidden />
        Clientes e fornecedores
      </NavLink>
      <NavLink to={`${base}/products`}>
        <BoxIcon size={18} aria-hidden />
        Produtos e serviços
      </NavLink>
      <NavLink to={`${base}/categories`}>
        <FolderTreeIcon size={18} aria-hidden />
        Categorias
      </NavLink>
      <NavLink to={`${base}/cost-centers`}>
        <LayersIcon size={18} aria-hidden />
        Centros de custo
      </NavLink>
      <NavLink to={`${base}/bank-accounts`}>
        <WalletIcon size={18} aria-hidden />
        Contas bancárias
      </NavLink>
      <NavLink to={`${base}/payment-methods`}>
        <CreditCardIcon size={18} aria-hidden />
        Formas de pagamento
      </NavLink>
      <NavLink to={`${base}/payment-terms`}>
        <CalendarDaysIcon size={18} aria-hidden />
        Prazos de pagamento
      </NavLink>
      {canSeeMembers && (
        <NavLink to={`${base}/members`}>
          <UserPlusIcon size={18} aria-hidden />
          Membros
        </NavLink>
      )}
    </nav>
  );
}
