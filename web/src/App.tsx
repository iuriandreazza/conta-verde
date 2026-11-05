import { Route, Routes } from "react-router-dom";
import { Layout } from "./components/Layout";
import { RequireAuth } from "./components/RequireAuth";
import { AuthPage } from "./pages/AuthPage";
import { CompaniesPage } from "./pages/CompaniesPage";
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
        </Route>
      </Route>
    </Routes>
  );
}
