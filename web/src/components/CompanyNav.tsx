import { NavLink } from "react-router-dom";

/** The registers of a company; the link of the page being shown is marked as current. */
export function CompanyNav({ companyId }: { companyId: string }) {
  return (
    <nav className="tabs" aria-label="Cadastros">
      <NavLink to={`/companies/${companyId}/parties`}>Clientes e fornecedores</NavLink>
      <NavLink to={`/companies/${companyId}/products`}>Produtos e serviços</NavLink>
      <NavLink to={`/companies/${companyId}/categories`}>Categorias</NavLink>
      <NavLink to={`/companies/${companyId}/cost-centers`}>Centros de custo</NavLink>
    </nav>
  );
}
