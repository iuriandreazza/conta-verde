import { Route, Routes } from "react-router-dom";
import { Layout } from "./components/Layout";
import { RequireAuth } from "./components/RequireAuth";
import { AcceptInvitationPage } from "./pages/AcceptInvitationPage";
import { ConfirmEmailPage } from "./pages/ConfirmEmailPage";
import { ForgotPasswordPage } from "./pages/ForgotPasswordPage";
import { ResetPasswordPage } from "./pages/ResetPasswordPage";
import { AuthPage } from "./pages/AuthPage";
import { BankAccountsPage } from "./pages/BankAccountsPage";
import { CashFlowPage } from "./pages/CashFlowPage";
import { CategoriesPage } from "./pages/CategoriesPage";
import { CompaniesPage } from "./pages/CompaniesPage";
import { AccountingPage } from "./pages/AccountingPage";
import { GatewaySettingsPage } from "./pages/GatewaySettingsPage";
import { PosPage } from "./pages/PosPage";
import { ReceivedInvoicesPage } from "./pages/ReceivedInvoicesPage";
import { FiscalDocumentsPage } from "./pages/FiscalDocumentsPage";
import { FiscalSettingsPage } from "./pages/FiscalSettingsPage";
import { CollectionPage } from "./pages/CollectionPage";
import { CommissionsPage } from "./pages/CommissionsPage";
import { PriceListsPage } from "./pages/PriceListsPage";
import { ContractsPage } from "./pages/ContractsPage";
import { CostCentersPage } from "./pages/CostCentersPage";
import { DashboardPage } from "./pages/DashboardPage";
import { DelinquencyPage } from "./pages/DelinquencyPage";
import { EntriesPage } from "./pages/EntriesPage";
import { InventoryCountsPage } from "./pages/InventoryCountsPage";
import { MembersPage } from "./pages/MembersPage";
import { NotFoundPage } from "./pages/NotFoundPage";
import { PartiesPage } from "./pages/PartiesPage";
import { PaymentPage } from "./pages/PaymentPage";
import { PaymentMethodsPage } from "./pages/PaymentMethodsPage";
import { PaymentTermsPage } from "./pages/PaymentTermsPage";
import { ProductsPage } from "./pages/ProductsPage";
import { PurchasesPage } from "./pages/PurchasesPage";
import { QuoteAnswerPage } from "./pages/QuoteAnswerPage";
import { QuotesPage } from "./pages/QuotesPage";
import { ReportsPage } from "./pages/ReportsPage";
import { ReconciliationPage } from "./pages/ReconciliationPage";
import { RecurrencesPage } from "./pages/RecurrencesPage";
import { SalesPage } from "./pages/SalesPage";
import { StockReportsPage } from "./pages/StockReportsPage";
import { StockPage } from "./pages/StockPage";
import { TransfersPage } from "./pages/TransfersPage";

export function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/login" element={<AuthPage mode="login" />} />
        <Route path="/register" element={<AuthPage mode="register" />} />
        <Route path="/confirm-email/:token" element={<ConfirmEmailPage />} />
        <Route path="/forgot-password" element={<ForgotPasswordPage />} />
        <Route path="/reset-password/:token" element={<ResetPasswordPage />} />
        <Route path="/invitations/:token" element={<AcceptInvitationPage />} />
        <Route path="/quote/:token" element={<QuoteAnswerPage />} />
        <Route path="/pay/:token" element={<PaymentPage />} />
        <Route element={<RequireAuth />}>
          <Route path="/" element={<CompaniesPage />} />
          <Route path="/companies/:companyId" element={<DashboardPage />} />
          <Route path="/companies/:companyId/parties" element={<PartiesPage />} />
          <Route path="/companies/:companyId/products" element={<ProductsPage />} />
          <Route path="/companies/:companyId/quotes" element={<QuotesPage />} />
          <Route path="/companies/:companyId/sales" element={<SalesPage />} />
          <Route path="/companies/:companyId/purchases" element={<PurchasesPage />} />
          <Route path="/companies/:companyId/stock" element={<StockPage />} />
          <Route path="/companies/:companyId/inventory-counts" element={<InventoryCountsPage />} />
          <Route path="/companies/:companyId/stock-reports" element={<StockReportsPage />} />
          <Route path="/companies/:companyId/contracts" element={<ContractsPage />} />
          <Route path="/companies/:companyId/commissions" element={<CommissionsPage />} />
          <Route path="/companies/:companyId/price-lists" element={<PriceListsPage />} />
          <Route path="/companies/:companyId/entries" element={<EntriesPage />} />
          <Route path="/companies/:companyId/accounting" element={<AccountingPage />} />
          <Route path="/companies/:companyId/pos" element={<PosPage />} />
          <Route path="/companies/:companyId/gateway" element={<GatewaySettingsPage />} />
          <Route path="/companies/:companyId/fiscal/received" element={<ReceivedInvoicesPage />} />
          <Route path="/companies/:companyId/fiscal/documents" element={<FiscalDocumentsPage />} />
          <Route path="/companies/:companyId/fiscal/settings" element={<FiscalSettingsPage />} />
          <Route path="/companies/:companyId/collection" element={<CollectionPage />} />
          <Route path="/companies/:companyId/delinquency" element={<DelinquencyPage />} />
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
