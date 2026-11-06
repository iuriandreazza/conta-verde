import { Route, Routes } from "react-router-dom";
import { Layout } from "./components/Layout";
import { RequireAuth } from "./components/RequireAuth";
import { AcceptInvitationPage } from "./pages/AcceptInvitationPage";
import { AuthPage } from "./pages/AuthPage";
import { BankAccountsPage } from "./pages/BankAccountsPage";
import { CashFlowPage } from "./pages/CashFlowPage";
import { CategoriesPage } from "./pages/CategoriesPage";
import { CompaniesPage } from "./pages/CompaniesPage";
import { ContractsPage } from "./pages/ContractsPage";
import { CostCentersPage } from "./pages/CostCentersPage";
import { DashboardPage } from "./pages/DashboardPage";
import { EntriesPage } from "./pages/EntriesPage";
import { MembersPage } from "./pages/MembersPage";
import { NotFoundPage } from "./pages/NotFoundPage";
import { PartiesPage } from "./pages/PartiesPage";
import { PaymentMethodsPage } from "./pages/PaymentMethodsPage";
import { PaymentTermsPage } from "./pages/PaymentTermsPage";
import { ProductsPage } from "./pages/ProductsPage";
import { QuoteAnswerPage } from "./pages/QuoteAnswerPage";
import { QuotesPage } from "./pages/QuotesPage";
import { ReportsPage } from "./pages/ReportsPage";
import { ReconciliationPage } from "./pages/ReconciliationPage";
import { RecurrencesPage } from "./pages/RecurrencesPage";
import { SalesPage } from "./pages/SalesPage";
import { TransfersPage } from "./pages/TransfersPage";

export function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/login" element={<AuthPage mode="login" />} />
        <Route path="/register" element={<AuthPage mode="register" />} />
        <Route path="/invitations/:token" element={<AcceptInvitationPage />} />
        <Route path="/quote/:token" element={<QuoteAnswerPage />} />
        <Route element={<RequireAuth />}>
          <Route path="/" element={<CompaniesPage />} />
          <Route path="/companies/:companyId" element={<DashboardPage />} />
          <Route path="/companies/:companyId/parties" element={<PartiesPage />} />
          <Route path="/companies/:companyId/products" element={<ProductsPage />} />
          <Route path="/companies/:companyId/quotes" element={<QuotesPage />} />
          <Route path="/companies/:companyId/sales" element={<SalesPage />} />
          <Route path="/companies/:companyId/contracts" element={<ContractsPage />} />
          <Route path="/companies/:companyId/entries" element={<EntriesPage />} />
          <Route path="/companies/:companyId/reports" element={<ReportsPage />} />
          <Route path="/companies/:companyId/reconciliation" element={<ReconciliationPage />} />
          <Route path="/companies/:companyId/recurrences" element={<RecurrencesPage />} />
          <Route path="/companies/:companyId/cash-flow" element={<CashFlowPage />} />
          <Route path="/companies/:companyId/transfers" element={<TransfersPage />} />
          <Route path="/companies/:companyId/categories" element={<CategoriesPage />} />
          <Route path="/companies/:companyId/cost-centers" element={<CostCentersPage />} />
          <Route path="/companies/:companyId/bank-accounts" element={<BankAccountsPage />} />
          <Route path="/companies/:companyId/payment-methods" element={<PaymentMethodsPage />} />
          <Route path="/companies/:companyId/payment-terms" element={<PaymentTermsPage />} />
          <Route path="/companies/:companyId/members" element={<MembersPage />} />
        </Route>
        <Route path="*" element={<NotFoundPage />} />
      </Route>
    </Routes>
  );
}
