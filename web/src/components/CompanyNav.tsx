import { NavLink } from "react-router-dom";
import { BoxIcon } from "./icons/box";
import { ChartLineIcon } from "./icons/chart-line";
import { FolderTreeIcon } from "./icons/folder-tree";
import { LayersIcon } from "./icons/layers";
import { UsersIcon } from "./icons/users";

/** The sections of a company; the link of the page being shown is marked as current. */
export function CompanyNav({ companyId }: { companyId: string }) {
  const base = `/companies/${companyId}`;
  return (
    <nav className="side-nav" aria-label="Seções da empresa">
      <NavLink to={base} end>
        <ChartLineIcon size={18} aria-hidden />
        Painel
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
    </nav>
  );
}
