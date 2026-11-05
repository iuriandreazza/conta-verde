import { Route, Routes } from "react-router-dom";
import { Layout } from "./components/Layout";
import { RequireAuth } from "./components/RequireAuth";
import { AuthPage } from "./pages/AuthPage";
import { CategoriesPage } from "./pages/CategoriesPage";
import { CompaniesPage } from "./pages/CompaniesPage";
import { CostCentersPage } from "./pages/CostCentersPage";
import { PartiesPage } from "./pages/PartiesPage";
import { ProductsPage } from "./pages/ProductsPage";

export function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/login" element={<AuthPage mode="login" />} />
        <Route path="/register" element={<AuthPage mode="register" />} />
        <Route element={<RequireAuth />}>
          <Route path="/" element={<CompaniesPage />} />
          <Route path="/companies/:companyId/parties" element={<PartiesPage />} />
          <Route path="/companies/:companyId/products" element={<ProductsPage />} />
          <Route path="/companies/:companyId/categories" element={<CategoriesPage />} />
          <Route path="/companies/:companyId/cost-centers" element={<CostCentersPage />} />
        </Route>
      </Route>
    </Routes>
  );
}
