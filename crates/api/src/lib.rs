mod access;
mod accounting;
mod api_keys;
mod attachments;
mod auth;
mod bank_accounts;
mod bank_connections;
mod cash_flow;
mod categories;
mod charges;
mod clock;
mod commissions;
mod connectors;
mod contracts;
mod cost_centers;
mod csv_import;
mod cursor;
mod delinquency;
mod documents;
mod dre;
mod dunning;
mod entries;
mod entries_import;
mod error;
mod export_files;
mod exports;
mod fiscal;
mod gateway_settings;
mod history;
mod inventory_counts;
mod json;
mod list_params;
mod members;
mod parties;
mod parties_import;
mod payment_methods;
mod payment_terms;
mod postal_codes;
mod price_lists;
mod products;
mod products_import;
mod purchase_imports;
mod purchase_orders;
mod quote_pdf;
mod quotes;
mod recurrences;
mod reports;
mod sales;
mod sales_reports;
mod session;
mod statements;
mod stock;
mod stock_reports;
mod transfers;
mod v1;
mod webhooks;

use std::sync::Arc;

use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use conta_verde_application::{
    AccountingExportService, ApiKeyRepository, ApiKeyService, AttachmentRepository,
    AttachmentService, AuthService, BalanceService, BankAccountRepository, BankAccountService,
    BankAggregator, BankConnectionRepository, BankSyncService, BlobStore, CategoryRepository,
    CategoryService, ChargeRepository, ChargeService, CnpjRegistry, CommissionRuleRepository,
    CommissionService, ContractRepository, ContractService, CostCenterRepository,
    CostCenterService, DocumentMailRepository, DocumentMailService, DocumentReferences, DreService,
    EmailSender, EntryLookups, EntryReferences, ExportLookups, FinancialEntryRepository,
    FinancialEntryService, FiscalDocumentRepository, FiscalDocumentService, FiscalGatewayResolver,
    FiscalSettingsRepository, FiscalSettingsService, GatewaySettingsRepository,
    GatewaySettingsService, IdentityRepository, InventoryCountRepository, InventoryCountService,
    InvitationRepository, JobQueue, MemberService, OpeningRepository, PackageScheduleRepository,
    PackageScheduleService, PartyRepository, PartyService, PaymentMethodRepository,
    PaymentMethodService, PaymentProvider, PaymentTermRepository, PaymentTermService,
    PostalCodeDirectory, PostalCodeService, PriceListRepository, PriceListService,
    ProductRepository, ProductService, PurchaseOrderRepository, PurchaseService, QuoteRepository,
    QuoteService, ReconciliationService, RecurrenceRepository, RecurrenceService,
    SaleClaimRepository, SaleRepository, SaleService, SalesReportService, SecretService,
    StatementLineRepository, StockMovementRepository, StockService, StockSettingsRepository,
    TransferRepository, TransferService, WebhookRepository, WebhookService,
};
use conta_verde_application::{
    CollectionService, CollectionSettingsRepository, DunningPlanRepository, DunningService,
    MessageSender, NotificationRepository, PaymentEventRepository, PaymentLinkRepository,
    PaymentPageService, PaymentWebhookService, PeriodLockRepository, PeriodLockService,
};
use serde::Serialize;

pub use v1::{V1Clock, V1Config, V1Events};

#[derive(Debug, Clone, Default)]
pub struct ApiConfig {
    /// Adds the `Secure` flag to the session cookie. Turn it on whenever the API is served over HTTPS.
    pub cookie_secure: bool,
    /// How the fiscal gateway may knock to say something happened ([ADR 0083], [ADR 0085]).
    pub fiscal_webhook: FiscalWebhook,
    /// The public `v1` API: how many requests a key may make and what time it is ([ADR 0087]).
    pub v1: V1Config,
}

/// The secret the fiscal gateway sends in a header of its own choosing: the notice it wakes the server with is believed for
/// nothing (the server asks the gateway), so this only keeps strangers from making the server ask. The header's name is the
/// operator's; the secret is the company's own when it set one, and this one (the operator's) otherwise. **Without either, the
/// door is shut.**
#[derive(Clone)]
pub struct FiscalWebhook {
    /// The name of the HTTP header the gateway puts the secret in.
    pub header: String,
    /// The operator's secret, for the companies that have set none of their own.
    pub secret: Option<String>,
}

impl Default for FiscalWebhook {
    fn default() -> Self {
        Self {
            header: DEFAULT_FISCAL_WEBHOOK_HEADER.to_owned(),
            secret: None,
        }
    }
}

/// The header the gateway puts the webhook secret in unless the operator chose another name.
pub const DEFAULT_FISCAL_WEBHOOK_HEADER: &str = "x-fiscal-webhook-token";

impl std::fmt::Debug for FiscalWebhook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FiscalWebhook")
            .field("header", &self.header)
            .field("secret", &self.secret.as_ref().map(|_| "<hidden>"))
            .finish()
    }
}

/// The adapters the use cases run on, named once so that a handler has one type parameter however
/// many registers there are. Generic rather than `dyn`, like the use cases themselves.
pub use clock::{customer_today, now_millis, now_unix, today};

pub trait Ports: 'static {
    type ApiKeys: ApiKeyRepository + 'static;
    type BankConnections: BankConnectionRepository + 'static;
    type Connectors: conta_verde_application::ConnectorRepository + 'static;
    type Aggregator: BankAggregator + 'static;
    type WebhookSubscriptions: WebhookRepository + 'static;
    type Jobs: JobQueue + 'static;
    type SaleClaims: SaleClaimRepository + 'static;
    type Parties: PartyRepository + 'static;
    type Registry: CnpjRegistry + 'static;
    type Identity: IdentityRepository + 'static;
    type Secrets: SecretService + 'static;
    type Products: ProductRepository + 'static;
    type CostCenters: CostCenterRepository + 'static;
    type Categories: CategoryRepository + 'static;
    type BankAccounts: BankAccountRepository + 'static;
    type PaymentMethods: PaymentMethodRepository + 'static;
    type PaymentTerms: PaymentTermRepository + 'static;
    type PriceLists: PriceListRepository + 'static;
    type Entries: FinancialEntryRepository + 'static;
    type EntryReferences: EntryReferences + EntryLookups + ExportLookups + 'static;
    type PeriodLocks: PeriodLockRepository + 'static;
    type PackageSchedules: PackageScheduleRepository + 'static;
    type FiscalSettings: FiscalSettingsRepository + 'static;
    type GatewaySettings: GatewaySettingsRepository + 'static;
    /// What seals the gateway settings' secrets ([ADR 0085]).
    type Cipher: conta_verde_application::SecretCipher + 'static;
    type Gateway: FiscalGatewayResolver + 'static;
    type FiscalDocuments: FiscalDocumentRepository + 'static;
    type Attachments: AttachmentRepository + 'static;
    type Blobs: BlobStore + 'static;
    type Charges: ChargeRepository + 'static;
    type CollectionSettings: CollectionSettingsRepository + 'static;
    type PaymentEvents: PaymentEventRepository + 'static;
    type PaymentLinks: PaymentLinkRepository + 'static;
    type DunningPlans: DunningPlanRepository + 'static;
    type Notifications: NotificationRepository + 'static;
    type Messages: MessageSender + 'static;
    type Provider: PaymentProvider + 'static;
    type Transfers: TransferRepository + 'static;
    type Openings: OpeningRepository + 'static;
    type Recurrences: RecurrenceRepository + 'static;
    type DocumentReferences: DocumentReferences + 'static;
    type Quotes: QuoteRepository + 'static;
    type Sales: SaleRepository + 'static;
    type Purchases: PurchaseOrderRepository + 'static;
    type StockMovements: StockMovementRepository + 'static;
    type StockSettings: StockSettingsRepository + 'static;
    type Counts: InventoryCountRepository + 'static;
    type Contracts: ContractRepository + 'static;
    type CommissionRules: CommissionRuleRepository + 'static;
    type Statements: StatementLineRepository + 'static;
    type PostalCodes: PostalCodeDirectory + 'static;
    type Invitations: InvitationRepository + 'static;
    type Emails: EmailSender + 'static;
    type DocumentMails: DocumentMailRepository + 'static;
}

/// The charges use case over the adapters of `X`.
type ChargesOf<X> = ChargeService<
    <X as Ports>::Charges,
    <X as Ports>::Entries,
    <X as Ports>::Parties,
    <X as Ports>::Provider,
    <X as Ports>::CollectionSettings,
>;

/// The reminders use case over the adapters of `X`.
type DunningOf<X> = DunningService<
    <X as Ports>::DunningPlans,
    <X as Ports>::Entries,
    <X as Ports>::Parties,
    PaymentPagesOf<X>,
    <X as Ports>::Notifications,
    <X as Ports>::Identity,
    <X as Ports>::Emails,
    <X as Ports>::Messages,
>;

/// The payment pages use case over the adapters of `X`.
type PaymentPagesOf<X> = PaymentPageService<
    <X as Ports>::PaymentLinks,
    <X as Ports>::Charges,
    <X as Ports>::Entries,
    <X as Ports>::Secrets,
>;

/// The webhooks use case over the adapters of `X`.
type WebhooksOf<X> = PaymentWebhookService<
    <X as Ports>::Provider,
    <X as Ports>::Charges,
    <X as Ports>::PaymentEvents,
    <X as Ports>::CollectionSettings,
    <X as Ports>::Entries,
    <X as Ports>::EntryReferences,
>;

/// The stock use case over the adapters of `X`.
type StockOf<X> = StockService<
    <X as Ports>::StockMovements,
    <X as Ports>::Products,
    <X as Ports>::StockSettings,
    <X as Ports>::PeriodLocks,
>;

/// The balances use case over the adapters of `X`.
type BalancesOf<X> = BalanceService<
    <X as Ports>::BankAccounts,
    <X as Ports>::Entries,
    <X as Ports>::Transfers,
    <X as Ports>::Openings,
    <X as Ports>::PeriodLocks,
>;

/// The schedule of the monthly package over the adapters of `X`.
type PackagesOf<X> = PackageScheduleService<
    <X as Ports>::PackageSchedules,
    <X as Ports>::Identity,
    <X as Ports>::Emails,
>;

/// The accountant's exports over the adapters of `X`.
type ExportsOf<X> = AccountingExportService<
    <X as Ports>::Entries,
    <X as Ports>::Transfers,
    <X as Ports>::Attachments,
    <X as Ports>::Blobs,
    <X as Ports>::EntryReferences,
>;

/// The inventory counts use case over the adapters of `X`.
type CountsOf<X> = InventoryCountService<<X as Ports>::Counts, StockOf<X>>;

/// The sales use case over the adapters of `X`.
type SalesOf<X> = SaleService<
    <X as Ports>::Sales,
    <X as Ports>::Quotes,
    <X as Ports>::Entries,
    <X as Ports>::EntryReferences,
    <X as Ports>::DocumentReferences,
    StockOf<X>,
>;

/// The purchase orders use case over the adapters of `X`.
/// The fiscal documents use case over the adapters of `X`.
type FiscalDocumentsOf<X> = FiscalDocumentService<
    <X as Ports>::FiscalDocuments,
    <X as Ports>::Sales,
    <X as Ports>::Parties,
    <X as Ports>::Products,
    <X as Ports>::FiscalSettings,
    <X as Ports>::Gateway,
    <X as Ports>::Blobs,
    <X as Ports>::EntryReferences,
>;

/// The e-mailing of documents over the adapters of `X`.
type DocumentsOf<X> = DocumentMailService<
    <X as Ports>::Purchases,
    <X as Ports>::Entries,
    <X as Ports>::Parties,
    <X as Ports>::Emails,
    <X as Ports>::DocumentMails,
>;

type PurchasesOf<X> = PurchaseService<
    <X as Ports>::Purchases,
    <X as Ports>::Entries,
    <X as Ports>::EntryReferences,
    <X as Ports>::DocumentReferences,
    StockOf<X>,
>;

/// The contracts use case over the adapters of `X`.
type ContractsOf<X> = ContractService<
    <X as Ports>::Contracts,
    <X as Ports>::Sales,
    <X as Ports>::Quotes,
    <X as Ports>::Entries,
    <X as Ports>::EntryReferences,
    <X as Ports>::DocumentReferences,
    StockOf<X>,
>;

/// The commissions use case over the adapters of `X`.
type CommissionsOf<X> = CommissionService<
    <X as Ports>::CommissionRules,
    <X as Ports>::Sales,
    <X as Ports>::DocumentReferences,
>;

/// The price lists use case over the adapters of `X`.
type PriceListsOf<X> = PriceListService<<X as Ports>::PriceLists, <X as Ports>::DocumentReferences>;

/// The sales reports use case over the adapters of `X`.
type SalesReportsOf<X> = SalesReportService<<X as Ports>::Sales>;

/// The subscriptions to webhooks and the sending of them ([ADR 0088]), shared with what raises the events.
pub type OutgoingHooksOf<X> = Arc<
    WebhookService<
        <X as Ports>::WebhookSubscriptions,
        <X as Ports>::Cipher,
        <X as Ports>::Jobs,
        <X as Ports>::Secrets,
    >,
>;

/// The API keys of the company, shared with the connectors that make them ([ADR 0090]).
pub type ApiKeysOf<X> = Arc<ApiKeyService<<X as Ports>::ApiKeys, <X as Ports>::Secrets>>;

/// The connectors, over the same key and subscription services as their own pages.
pub type ConnectorsOf<X> = conta_verde_application::ConnectorService<
    <X as Ports>::Connectors,
    ApiKeysOf<X>,
    OutgoingHooksOf<X>,
>;

/// The bank connections and the pull of their statements ([ADR 0089]), shared with the worker that pulls.
pub type BankSyncOf<X> = Arc<
    BankSyncService<
        <X as Ports>::BankConnections,
        <X as Ports>::Aggregator,
        ReconciliationService<
            <X as Ports>::Statements,
            <X as Ports>::Entries,
            <X as Ports>::Transfers,
            <X as Ports>::EntryReferences,
        >,
        <X as Ports>::Jobs,
    >,
>;

/// The use cases the API serves.
pub struct Services<X: Ports> {
    pub bank_sync: BankSyncOf<X>,
    pub webhook_subscriptions: OutgoingHooksOf<X>,
    pub api_keys: ApiKeysOf<X>,
    pub connectors: X::Connectors,
    pub sale_claims: X::SaleClaims,
    pub parties: PartyService<X::Parties, X::Registry>,
    pub products: ProductService<X::Products>,
    pub cost_centers: CostCenterService<X::CostCenters>,
    pub categories: CategoryService<X::Categories>,
    pub bank_accounts: BankAccountService<X::BankAccounts>,
    pub payment_methods: PaymentMethodService<X::PaymentMethods>,
    pub payment_terms: PaymentTermService<X::PaymentTerms>,
    pub price_lists: PriceListsOf<X>,
    pub sales_reports: SalesReportsOf<X>,
    pub entries: FinancialEntryService<X::Entries, X::EntryReferences>,
    pub dre: DreService<X::Entries, X::Categories>,
    pub attachments: AttachmentService<X::Attachments, X::Blobs, X::Entries>,
    pub charges: ChargesOf<X>,
    pub webhooks: WebhooksOf<X>,
    pub payment_pages: PaymentPagesOf<X>,
    pub dunning: DunningOf<X>,
    pub collection: CollectionService<X::CollectionSettings, X::EntryReferences>,
    pub transfers: TransferService<X::Transfers, X::EntryReferences>,
    pub recurrences: RecurrenceService<X::Recurrences, X::Entries, X::EntryReferences>,
    pub quotes: QuoteService<X::Quotes, X::DocumentReferences, X::Emails, X::Secrets>,
    pub sales: SalesOf<X>,
    pub purchases: PurchasesOf<X>,
    pub stock: StockOf<X>,
    pub counts: CountsOf<X>,
    pub contracts: ContractsOf<X>,
    pub commissions: CommissionsOf<X>,
    pub reconciliation:
        ReconciliationService<X::Statements, X::Entries, X::Transfers, X::EntryReferences>,
    pub balances: BalancesOf<X>,
    pub period_locks: PeriodLockService<X::PeriodLocks>,
    pub exports: ExportsOf<X>,
    pub packages: PackagesOf<X>,
    pub fiscal_settings: FiscalSettingsService<X::FiscalSettings>,
    pub gateway_settings: GatewaySettingsService<X::GatewaySettings, X::Cipher>,
    pub fiscal_documents: FiscalDocumentsOf<X>,
    pub postal_codes: PostalCodeService<X::PostalCodes>,
    pub members: MemberService<X::Invitations, X::Identity, X::Emails, X::Secrets>,
    pub documents: DocumentsOf<X>,
    pub auth: AuthService<X::Identity, X::Secrets>,
}

pub(crate) struct AppState<X: Ports> {
    bank_sync: BankSyncOf<X>,
    webhook_subscriptions: OutgoingHooksOf<X>,
    api_keys: ApiKeysOf<X>,
    connectors: ConnectorsOf<X>,
    sale_claims: X::SaleClaims,
    limiter: v1::RateLimiter,
    parties: PartyService<X::Parties, X::Registry>,
    products: ProductService<X::Products>,
    cost_centers: CostCenterService<X::CostCenters>,
    categories: CategoryService<X::Categories>,
    bank_accounts: BankAccountService<X::BankAccounts>,
    payment_methods: PaymentMethodService<X::PaymentMethods>,
    payment_terms: PaymentTermService<X::PaymentTerms>,
    price_lists: PriceListsOf<X>,
    sales_reports: SalesReportsOf<X>,
    entries: FinancialEntryService<X::Entries, X::EntryReferences>,
    dre: DreService<X::Entries, X::Categories>,
    attachments: AttachmentService<X::Attachments, X::Blobs, X::Entries>,
    charges: ChargesOf<X>,
    webhooks: WebhooksOf<X>,
    payment_pages: PaymentPagesOf<X>,
    dunning: DunningOf<X>,
    collection: CollectionService<X::CollectionSettings, X::EntryReferences>,
    transfers: TransferService<X::Transfers, X::EntryReferences>,
    recurrences: RecurrenceService<X::Recurrences, X::Entries, X::EntryReferences>,
    quotes: QuoteService<X::Quotes, X::DocumentReferences, X::Emails, X::Secrets>,
    sales: SalesOf<X>,
    purchases: PurchasesOf<X>,
    stock: StockOf<X>,
    counts: CountsOf<X>,
    contracts: ContractsOf<X>,
    commissions: CommissionsOf<X>,
    reconciliation:
        ReconciliationService<X::Statements, X::Entries, X::Transfers, X::EntryReferences>,
    balances: BalancesOf<X>,
    period_locks: PeriodLockService<X::PeriodLocks>,
    exports: ExportsOf<X>,
    packages: PackagesOf<X>,
    fiscal_settings: FiscalSettingsService<X::FiscalSettings>,
    gateway_settings: GatewaySettingsService<X::GatewaySettings, X::Cipher>,
    fiscal_documents: FiscalDocumentsOf<X>,
    postal_codes: PostalCodeService<X::PostalCodes>,
    members: MemberService<X::Invitations, X::Identity, X::Emails, X::Secrets>,
    documents: DocumentsOf<X>,
    auth: AuthService<X::Identity, X::Secrets>,
    config: ApiConfig,
}

/// What every handler receives.
pub(crate) type SharedState<X> = Arc<AppState<X>>;

pub fn router<X: Ports>(services: Services<X>, config: ApiConfig) -> Router {
    let limiter = v1::RateLimiter::new(config.v1.rate_limit_per_minute);
    let connectors = conta_verde_application::ConnectorService::new(
        services.connectors,
        services.api_keys.clone(),
        services.webhook_subscriptions.clone(),
    );
    let state = Arc::new(AppState {
        bank_sync: services.bank_sync,
        webhook_subscriptions: services.webhook_subscriptions,
        connectors,
        api_keys: services.api_keys,
        sale_claims: services.sale_claims,
        limiter,
        parties: services.parties,
        products: services.products,
        cost_centers: services.cost_centers,
        categories: services.categories,
        bank_accounts: services.bank_accounts,
        payment_methods: services.payment_methods,
        payment_terms: services.payment_terms,
        price_lists: services.price_lists,
        sales_reports: services.sales_reports,
        entries: services.entries,
        dre: services.dre,
        attachments: services.attachments,
        charges: services.charges,
        webhooks: services.webhooks,
        payment_pages: services.payment_pages,
        dunning: services.dunning,
        collection: services.collection,
        transfers: services.transfers,
        recurrences: services.recurrences,
        quotes: services.quotes,
        sales: services.sales,
        purchases: services.purchases,
        stock: services.stock,
        counts: services.counts,
        contracts: services.contracts,
        commissions: services.commissions,
        reconciliation: services.reconciliation,
        balances: services.balances,
        period_locks: services.period_locks,
        exports: services.exports,
        packages: services.packages,
        fiscal_settings: services.fiscal_settings,
        gateway_settings: services.gateway_settings,
        fiscal_documents: services.fiscal_documents,
        postal_codes: services.postal_codes,
        members: services.members,
        documents: services.documents,
        auth: services.auth,
        config,
    });
    Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(auth::register::<X>))
        .route("/auth/login", post(auth::login::<X>))
        .route("/auth/logout", post(auth::logout::<X>))
        .route("/auth/me", get(auth::me::<X>))
        .route(
            "/auth/email-confirmation",
            post(auth::request_email_confirmation::<X>),
        )
        .route(
            "/auth/email-confirmation/confirm",
            post(auth::confirm_email::<X>),
        )
        .route(
            "/auth/password-reset",
            post(auth::request_password_reset::<X>),
        )
        .route(
            "/auth/password-reset/confirm",
            post(auth::reset_password::<X>),
        )
        .route(
            "/companies/{company_id}/connectors",
            get(connectors::list_connectors::<X>).post(connectors::create_connector::<X>),
        )
        .route(
            "/companies/{company_id}/connectors/{connector_id}/activate",
            post(connectors::activate_connector::<X>),
        )
        .route(
            "/companies/{company_id}/connectors/{connector_id}/deactivate",
            post(connectors::deactivate_connector::<X>),
        )
        .route(
            "/companies/{company_id}/connectors/{connector_id}/history",
            get(connectors::connector_history::<X>),
        )
        .route(
            "/companies/{company_id}/bank-connections",
            get(bank_connections::list_connections::<X>).post(bank_connections::connect::<X>),
        )
        .route(
            "/companies/{company_id}/bank-connections/connect-token",
            post(bank_connections::connect_token::<X>),
        )
        .route(
            "/companies/{company_id}/bank-connections/accounts",
            post(bank_connections::aggregator_accounts::<X>),
        )
        .route(
            "/companies/{company_id}/bank-connections/{connection_id}/sync",
            post(bank_connections::sync_now::<X>),
        )
        .route(
            "/companies/{company_id}/bank-connections/{connection_id}/disconnect",
            post(bank_connections::disconnect::<X>),
        )
        .route(
            "/companies/{company_id}/bank-connections/{connection_id}/history",
            get(bank_connections::connection_history::<X>),
        )
        .route(
            "/companies/{company_id}/webhook-subscriptions",
            get(webhooks::list_subscriptions::<X>).post(webhooks::create_subscription::<X>),
        )
        .route(
            "/companies/{company_id}/webhook-subscriptions/{subscription_id}",
            put(webhooks::update_subscription::<X>),
        )
        .route(
            "/companies/{company_id}/webhook-subscriptions/{subscription_id}/secret",
            post(webhooks::replace_secret::<X>),
        )
        .route(
            "/companies/{company_id}/webhook-subscriptions/{subscription_id}/test",
            post(webhooks::send_test::<X>),
        )
        .route(
            "/companies/{company_id}/webhook-subscriptions/{subscription_id}/deliveries",
            get(webhooks::list_deliveries::<X>),
        )
        .route(
            "/companies/{company_id}/webhook-subscriptions/{subscription_id}/deliveries/{delivery_id}/redeliver",
            post(webhooks::redeliver::<X>),
        )
        .route(
            "/companies/{company_id}/webhook-subscriptions/{subscription_id}/history",
            get(webhooks::subscription_history::<X>),
        )
        .route(
            "/companies/{company_id}/api-keys",
            get(api_keys::list_api_keys::<X>).post(api_keys::create_api_key::<X>),
        )
        .route(
            "/companies/{company_id}/api-keys/{key_id}/rotate",
            post(api_keys::rotate_api_key::<X>),
        )
        .route(
            "/companies/{company_id}/api-keys/{key_id}/revoke",
            post(api_keys::revoke_api_key::<X>),
        )
        .route(
            "/companies/{company_id}/api-keys/{key_id}/history",
            get(api_keys::api_key_history::<X>),
        )
        .route(
            "/companies/{company_id}/parties",
            get(parties::list_parties::<X>).post(parties::register_party::<X>),
        )
        .route(
            "/companies/{company_id}/parties/import",
            post(parties_import::import_parties::<X>),
        )
        .route(
            "/companies/{company_id}/parties/{party_id}",
            put(parties::update_party::<X>),
        )
        .route(
            "/companies/{company_id}/parties/{party_id}/document-check",
            post(parties::verify_document::<X>),
        )
        .route(
            "/companies/{company_id}/parties/{party_id}/history",
            get(parties::party_history::<X>),
        )
        .route(
            "/companies/{company_id}/products",
            get(products::list_products::<X>).post(products::register_product::<X>),
        )
        .route(
            "/companies/{company_id}/products/import",
            post(products_import::import_products::<X>),
        )
        .route(
            "/companies/{company_id}/products/{product_id}",
            put(products::update_product::<X>),
        )
        .route(
            "/companies/{company_id}/products/{product_id}/history",
            get(products::product_history::<X>),
        )
        .route(
            "/companies/{company_id}/cost-centers",
            get(cost_centers::list_cost_centers::<X>).post(cost_centers::register_cost_center::<X>),
        )
        .route(
            "/companies/{company_id}/cost-centers/{cost_center_id}",
            put(cost_centers::update_cost_center::<X>),
        )
        .route(
            "/companies/{company_id}/cost-centers/{cost_center_id}/history",
            get(cost_centers::cost_center_history::<X>),
        )
        .route(
            "/companies/{company_id}/categories",
            get(categories::list_categories::<X>).post(categories::create_category::<X>),
        )
        .route(
            "/companies/{company_id}/categories/default-template",
            post(categories::apply_default_template::<X>),
        )
        .route(
            "/companies/{company_id}/categories/{category_id}",
            put(categories::change_category::<X>),
        )
        .route(
            "/companies/{company_id}/categories/{category_id}/history",
            get(categories::category_history::<X>),
        )
        .route(
            "/companies/{company_id}/bank-accounts",
            get(bank_accounts::list_bank_accounts::<X>)
                .post(bank_accounts::register_bank_account::<X>),
        )
        .route(
            "/companies/{company_id}/bank-accounts/{bank_account_id}",
            put(bank_accounts::update_bank_account::<X>),
        )
        .route(
            "/companies/{company_id}/bank-accounts/{bank_account_id}/history",
            get(bank_accounts::bank_account_history::<X>),
        )
        .route(
            "/companies/{company_id}/payment-methods",
            get(payment_methods::list_payment_methods::<X>)
                .post(payment_methods::register_payment_method::<X>),
        )
        .route(
            "/companies/{company_id}/payment-methods/{payment_method_id}",
            put(payment_methods::update_payment_method::<X>),
        )
        .route(
            "/companies/{company_id}/payment-methods/{payment_method_id}/history",
            get(payment_methods::payment_method_history::<X>),
        )
        .route(
            "/companies/{company_id}/payment-terms",
            get(payment_terms::list_payment_terms::<X>)
                .post(payment_terms::register_payment_term::<X>),
        )
        .route(
            "/companies/{company_id}/payment-terms/{payment_term_id}",
            put(payment_terms::update_payment_term::<X>),
        )
        .route(
            "/companies/{company_id}/payment-terms/{payment_term_id}/history",
            get(payment_terms::payment_term_history::<X>),
        )
        .route(
            "/companies/{company_id}/price-lists",
            get(price_lists::list_price_lists::<X>).post(price_lists::register_price_list::<X>),
        )
        .route(
            "/companies/{company_id}/price-lists/{price_list_id}",
            put(price_lists::update_price_list::<X>),
        )
        .route(
            "/companies/{company_id}/price-lists/{price_list_id}/history",
            get(price_lists::price_list_history::<X>),
        )
        .route(
            "/companies/{company_id}/entries",
            get(entries::list_entries::<X>).post(entries::register_entry::<X>),
        )
        .route(
            "/companies/{company_id}/entries/import",
            post(entries_import::import_entries::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}",
            get(entries::get_entry::<X>).put(entries::update_entry::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}/history",
            get(entries::entry_history::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}/settlements",
            get(entries::list_settlements::<X>).post(entries::settle_installment::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}/settlements/{settlement_id}/receipt",
            post(documents::send_payment_receipt::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}/settlements/{settlement_id}/mails",
            get(documents::payment_receipt_mails::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}/settlements/{settlement_id}",
            delete(entries::reverse_settlement::<X>),
        )
        .route(
            "/companies/{company_id}/bank-accounts/{bank_account_id}/statement-import",
            post(statements::import_statement::<X>),
        )
        .route(
            "/companies/{company_id}/statement-lines",
            get(statements::list_statement_lines::<X>),
        )
        .route(
            "/companies/{company_id}/statement-lines/{line_id}/suggestions",
            get(statements::statement_line_suggestions::<X>),
        )
        .route(
            "/companies/{company_id}/statement-lines/{line_id}/match",
            post(statements::match_statement_line::<X>),
        )
        .route(
            "/companies/{company_id}/statement-lines/{line_id}/unmatch",
            post(statements::unmatch_statement_line::<X>),
        )
        .route(
            "/companies/{company_id}/statement-lines/{line_id}/ignore",
            post(statements::ignore_statement_line::<X>),
        )
        .route(
            "/companies/{company_id}/statement-lines/{line_id}/restore",
            post(statements::restore_statement_line::<X>),
        )
        .route(
            "/companies/{company_id}/statement-lines/{line_id}/create-entry",
            post(statements::create_entry_from_statement_line::<X>),
        )
        .route(
            "/companies/{company_id}/statement-lines/{line_id}/history",
            get(statements::statement_line_history::<X>),
        )
        .route(
            "/companies/{company_id}/reports/entries",
            get(reports::entries_report::<X>),
        )
        .route(
            "/companies/{company_id}/reports/overdue",
            get(reports::overdue_report::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}/attachments",
            get(attachments::list_attachments::<X>).post(attachments::attach::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}/attachments/{attachment_id}",
            get(attachments::download_attachment::<X>).delete(attachments::remove_attachment::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}/charges",
            get(charges::list_charges::<X>).post(charges::issue_charge::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}/charges/{charge_id}/cancel",
            post(charges::cancel_charge::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}/charges/{charge_id}/link",
            post(charges::new_payment_link::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}/charges/{charge_id}/history",
            get(charges::charge_history::<X>),
        )
        .route(
            "/companies/{company_id}/collection",
            get(charges::collection::<X>),
        )
        .route(
            "/companies/{company_id}/collection/settings",
            get(charges::get_collection_settings::<X>)
                .put(charges::replace_collection_settings::<X>),
        )
        .route(
            "/companies/{company_id}/accounting/period-lock",
            get(accounting::get_period_lock::<X>).put(accounting::put_period_lock::<X>),
        )
        .route(
            "/companies/{company_id}/accounting/period-lock/history",
            get(accounting::period_lock_history::<X>),
        )
        .route(
            "/companies/{company_id}/accounting/package-schedule",
            get(accounting::get_package_schedule::<X>).put(accounting::put_package_schedule::<X>),
        )
        .route(
            "/companies/{company_id}/accounting/package-schedule/history",
            get(accounting::package_schedule_history::<X>),
        )
        .route(
            "/companies/{company_id}/gateway-settings",
            get(gateway_settings::get_gateway_settings::<X>)
                .put(gateway_settings::replace_gateway_settings::<X>)
                .delete(gateway_settings::reset_gateway_settings::<X>),
        )
        .route(
            "/companies/{company_id}/gateway-settings/history",
            get(gateway_settings::gateway_settings_history::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/gateway",
            get(fiscal::get_gateway::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/settings",
            get(fiscal::get_settings::<X>).put(fiscal::replace_settings::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/settings/history",
            get(fiscal::settings_history::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/documents",
            get(fiscal::list_documents::<X>).post(fiscal::issue_document::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/received-invoices",
            get(fiscal::list_received::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/received-invoices/sync",
            post(fiscal::sync_received::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/received-invoices/{id}",
            get(fiscal::get_received::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/received-invoices/{id}/history",
            get(fiscal::received_history::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/received-invoices/{id}/manifest",
            post(fiscal::manifest_received::<X>),
        )
        .route(
            "/public/fiscal/{company_id}/webhook",
            post(fiscal::receive_fiscal_webhook::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/service-documents",
            post(fiscal::issue_service_document::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/consumer-documents",
            post(fiscal::issue_consumer_document::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/documents/{id}",
            get(fiscal::get_document::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/documents/{id}/history",
            get(fiscal::document_history::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/documents/{id}/refresh",
            post(fiscal::refresh_document::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/documents/{id}/resend",
            post(fiscal::resend_document::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/documents/{id}/resend-service",
            post(fiscal::resend_service_document::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/documents/{id}/resend-consumer",
            post(fiscal::resend_consumer_document::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/documents/{id}/cancel",
            post(fiscal::cancel_document::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/documents/{id}/corrections",
            get(fiscal::list_corrections::<X>).post(fiscal::send_correction::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/invalidations",
            get(fiscal::list_invalidations::<X>).post(fiscal::invalidate_numbers::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/events/{id}/abandon",
            post(fiscal::abandon_event::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/events/{id}/file",
            get(fiscal::download_event_file::<X>),
        )
        .route(
            "/companies/{company_id}/fiscal/documents/{id}/files/{kind}",
            get(fiscal::download_document_file::<X>),
        )
        .route(
            "/companies/{company_id}/accounting/exports/entries",
            get(exports::entries_file::<X>),
        )
        .route(
            "/companies/{company_id}/accounting/exports/settlements",
            get(exports::settlements_file::<X>),
        )
        .route(
            "/companies/{company_id}/accounting/exports/transfers",
            get(exports::transfers_file::<X>),
        )
        .route(
            "/companies/{company_id}/accounting/exports/package",
            get(exports::package_file::<X>),
        )
        .route(
            "/companies/{company_id}/dre",
            get(dre::income_statement::<X>),
        )
        .route(
            "/companies/{company_id}/delinquency",
            get(delinquency::delinquency::<X>),
        )
        .route(
            "/companies/{company_id}/collection/dunning",
            get(dunning::get_plan::<X>).put(dunning::replace_plan::<X>),
        )
        .route(
            "/companies/{company_id}/collection/dunning/history",
            get(dunning::plan_history::<X>),
        )
        .route(
            "/companies/{company_id}/entries/{entry_id}/notifications",
            get(dunning::list_notifications::<X>),
        )
        .route(
            "/companies/{company_id}/collection/settings/history",
            get(charges::collection_settings_history::<X>),
        )
        .route(
            "/public/payments/{company_id}/webhooks/{provider}",
            post(charges::receive_payment_webhook::<X>),
        )
        .route(
            "/companies/{company_id}/recurrences",
            get(recurrences::list_recurrences::<X>).post(recurrences::register_recurrence::<X>),
        )
        .route(
            "/companies/{company_id}/recurrences/{recurrence_id}",
            put(recurrences::update_recurrence::<X>),
        )
        .route(
            "/companies/{company_id}/recurrences/{recurrence_id}/history",
            get(recurrences::recurrence_history::<X>),
        )
        .route(
            "/companies/{company_id}/transfers",
            get(transfers::list_transfers::<X>).post(transfers::register_transfer::<X>),
        )
        .route(
            "/companies/{company_id}/transfers/{transfer_id}",
            delete(transfers::reverse_transfer::<X>),
        )
        .route(
            "/companies/{company_id}/bank-accounts/{bank_account_id}/opening-balance",
            put(transfers::set_opening_balance::<X>),
        )
        .route(
            "/companies/{company_id}/cash-flow",
            get(cash_flow::cash_flow::<X>),
        )
        .route(
            "/companies/{company_id}/balances",
            get(transfers::balances::<X>),
        )
        .route(
            "/companies/{company_id}/members",
            get(members::list_members::<X>),
        )
        .route(
            "/companies/{company_id}/members/{user_id}",
            patch(members::change_role::<X>).delete(members::remove_member::<X>),
        )
        .route(
            "/companies/{company_id}/invitations",
            get(members::list_invitations::<X>).post(members::invite::<X>),
        )
        .route(
            "/companies/{company_id}/invitations/{invitation_id}",
            delete(members::revoke_invitation::<X>),
        )
        .route(
            "/companies/{company_id}/quotes",
            get(quotes::list_quotes::<X>).post(quotes::register_quote::<X>),
        )
        .route(
            "/companies/{company_id}/quotes/{quote_id}",
            get(quotes::get_quote::<X>).put(quotes::update_quote::<X>),
        )
        .route(
            "/companies/{company_id}/quotes/{quote_id}/history",
            get(quotes::quote_history::<X>),
        )
        .route(
            "/companies/{company_id}/quotes/{quote_id}/send",
            post(quotes::send_quote::<X>),
        )
        .route(
            "/companies/{company_id}/quotes/{quote_id}/pdf",
            get(quotes::quote_pdf::<X>),
        )
        .route(
            "/companies/{company_id}/quotes/{quote_id}/convert",
            post(sales::convert_quote::<X>),
        )
        .route(
            "/companies/{company_id}/commission-rules",
            get(commissions::list_rules::<X>).post(commissions::register_rule::<X>),
        )
        .route(
            "/companies/{company_id}/commission-rules/{rule_id}",
            put(commissions::update_rule::<X>),
        )
        .route(
            "/companies/{company_id}/commission-rules/{rule_id}/history",
            get(commissions::rule_history::<X>),
        )
        .route(
            "/companies/{company_id}/sales-reports",
            get(sales_reports::sales_report::<X>),
        )
        .route(
            "/companies/{company_id}/commissions",
            get(commissions::commission_statement::<X>),
        )
        .route(
            "/companies/{company_id}/contracts",
            get(contracts::list_contracts::<X>).post(contracts::register_contract::<X>),
        )
        .route(
            "/companies/{company_id}/contracts/{contract_id}",
            get(contracts::get_contract::<X>).put(contracts::update_contract::<X>),
        )
        .route(
            "/companies/{company_id}/contracts/{contract_id}/history",
            get(contracts::contract_history::<X>),
        )
        .route(
            "/companies/{company_id}/contracts/{contract_id}/adjust",
            post(contracts::adjust_contract::<X>),
        )
        .route(
            "/companies/{company_id}/contracts/{contract_id}/terminate",
            post(contracts::terminate_contract::<X>),
        )
        .route(
            "/companies/{company_id}/sales",
            get(sales::list_sales::<X>).post(sales::register_sale::<X>),
        )
        .route(
            "/companies/{company_id}/sales/{sale_id}",
            get(sales::get_sale::<X>).put(sales::update_sale::<X>),
        )
        .route(
            "/companies/{company_id}/sales/{sale_id}/history",
            get(sales::sale_history::<X>),
        )
        .route(
            "/companies/{company_id}/sales/{sale_id}/complete",
            post(sales::complete_sale::<X>),
        )
        .route(
            "/companies/{company_id}/sales/{sale_id}/cancel",
            post(sales::cancel_sale::<X>),
        )
        .route(
            "/companies/{company_id}/sales/{sale_id}/returns",
            get(sales::list_returns::<X>).post(sales::register_return::<X>),
        )
        .route(
            "/companies/{company_id}/stock/movements",
            get(stock::list_movements::<X>).post(stock::record_movement::<X>),
        )
        .route(
            "/companies/{company_id}/stock/movements/kind-counts",
            get(stock::movement_kind_counts::<X>),
        )
        .route(
            "/companies/{company_id}/stock/movements/{movement_id}",
            get(stock::get_movement::<X>),
        )
        .route(
            "/companies/{company_id}/stock/movements/{movement_id}/reverse",
            post(stock::reverse_movement::<X>),
        )
        .route(
            "/companies/{company_id}/stock/positions",
            get(stock::list_positions::<X>),
        )
        .route(
            "/companies/{company_id}/stock/positions/summary",
            get(stock::position_summary::<X>),
        )
        .route(
            "/companies/{company_id}/stock/alerts",
            get(stock::list_alerts::<X>),
        )
        .route(
            "/companies/{company_id}/stock/settings",
            get(stock::get_settings::<X>).put(stock::put_settings::<X>),
        )
        .route(
            "/companies/{company_id}/stock/settings/history",
            get(stock::settings_history::<X>),
        )
        .route(
            "/companies/{company_id}/stock-reports/valuation",
            get(stock_reports::valuation::<X>),
        )
        .route(
            "/companies/{company_id}/stock-reports/movements",
            get(stock_reports::movements::<X>),
        )
        .route(
            "/companies/{company_id}/inventory-counts",
            get(inventory_counts::list_counts::<X>).post(inventory_counts::register_count::<X>),
        )
        .route(
            "/companies/{company_id}/inventory-counts/{count_id}",
            get(inventory_counts::get_count::<X>).put(inventory_counts::update_count::<X>),
        )
        .route(
            "/companies/{company_id}/inventory-counts/{count_id}/history",
            get(inventory_counts::count_history::<X>),
        )
        .route(
            "/companies/{company_id}/inventory-counts/{count_id}/apply",
            post(inventory_counts::apply_count::<X>),
        )
        .route(
            "/companies/{company_id}/inventory-counts/{count_id}/cancel",
            post(inventory_counts::cancel_count::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-imports",
            post(purchase_imports::enter_invoice::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-imports/preview",
            post(purchase_imports::preview_invoice::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-orders",
            get(purchase_orders::list_purchase_orders::<X>)
                .post(purchase_orders::register_purchase_order::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-orders/status-counts",
            get(purchase_orders::purchase_order_status_counts::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-orders/{order_id}",
            get(purchase_orders::get_purchase_order::<X>)
                .put(purchase_orders::update_purchase_order::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-orders/{order_id}/history",
            get(purchase_orders::purchase_order_history::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-orders/{order_id}/send",
            post(documents::send_purchase_order::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-orders/{order_id}/mails",
            get(documents::purchase_order_mails::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-orders/{order_id}/approve",
            post(purchase_orders::approve_purchase_order::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-orders/{order_id}/cancel",
            post(purchase_orders::cancel_purchase_order::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-orders/{order_id}/close",
            post(purchase_orders::close_purchase_order::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-orders/{order_id}/receipts",
            get(purchase_orders::list_receipts::<X>).post(purchase_orders::register_receipt::<X>),
        )
        .route(
            "/companies/{company_id}/purchase-orders/{order_id}/receipts/{receipt_id}/finish",
            post(purchase_orders::finish_receipt::<X>),
        )
        .route(
            "/invitations/{token}",
            get(members::preview_invitation::<X>),
        )
        .route(
            "/invitations/{token}/accept",
            post(members::accept_invitation::<X>),
        )
        .route(
            "/companies/{company_id}/postal-codes/{code}",
            get(postal_codes::find_postal_code::<X>),
        )
        .merge(
            Router::new()
                .route("/public/quotes/{token}", get(quotes::public_quote::<X>))
                .route("/public/charges/{token}", get(charges::public_charge::<X>))
                .route(
                    "/public/quotes/{token}/pdf",
                    get(quotes::public_quote_pdf::<X>),
                )
                .route(
                    "/public/quotes/{token}/approve",
                    post(quotes::approve_quote::<X>),
                )
                .route(
                    "/public/quotes/{token}/decline",
                    post(quotes::decline_quote::<X>),
                )
                .layer(axum::middleware::map_response(quotes::keep_private)),
        )
        .merge(v1::routes(state.clone()))
        .with_state(state)
        // The outermost layer: every request gets a company scope of its own, empty until the caller is known
        // ([ADR 0093]).
        .layer(axum::middleware::from_fn(tenant_scope))
}

/// What the row-level policies know of a request: nothing, until the extractor that checks the caller's membership
/// (or the gate that checks an API key, or the lookup of a token that names a company) narrows it to one company.
async fn tenant_scope(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    conta_verde_infrastructure::tenant::scoped(next.run(request)).await
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use conta_verde_application::{BankSyncService, PostalCodeEntry};
    use conta_verde_infrastructure::{
        Argon2SecretService, FakeFiscalGateway, FakePaymentProvider, InMemoryApiKeyRepository,
        InMemoryAttachmentRepository, InMemoryBankAccountRepository, InMemoryBlobStore,
        InMemoryCategoryRepository, InMemoryChargeRepository, InMemoryCnpjRegistry,
        InMemoryCollectionSettingsRepository, InMemoryCommissionRuleRepository,
        InMemoryContractRepository, InMemoryCostCenterRepository, InMemoryDocumentMailRepository,
        InMemoryDunningPlanRepository, InMemoryEmailSender, InMemoryFinancialEntryRepository,
        InMemoryFiscalDocumentRepository, InMemoryFiscalSettingsRepository,
        InMemoryGatewaySettingsRepository, InMemoryIdentityRepository,
        InMemoryInventoryCountRepository, InMemoryInvitationRepository, InMemoryJobQueue,
        InMemoryMessageSender, InMemoryNotificationRepository, InMemoryOpeningRepository,
        InMemoryPackageScheduleRepository, InMemoryPartyRepository, InMemoryPaymentEventRepository,
        InMemoryPaymentLinkRepository, InMemoryPaymentMethodRepository,
        InMemoryPaymentTermRepository, InMemoryPeriodLockRepository, InMemoryPostalCodeDirectory,
        InMemoryPriceListRepository, InMemoryProductRepository, InMemoryPurchaseOrderRepository,
        InMemoryQuoteRepository, InMemoryRecurrenceRepository, InMemorySaleClaimRepository,
        InMemorySaleRepository, InMemoryStatementLineRepository, InMemoryStockMovementRepository,
        InMemoryStockSettingsRepository, InMemoryTransferRepository, InMemoryWebhookRepository,
        RegisterDocumentReferences, RegisterReferences, RingSecretCipher,
    };
    use conta_verde_infrastructure::{
        FakeBankAggregator, InMemoryBankConnectionRepository, InMemoryConnectorRepository,
    };
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::*;

    /// The registry knows the one company the tests register, and a closed alphanumeric one.
    fn known_companies() -> InMemoryCnpjRegistry {
        InMemoryCnpjRegistry::default()
            .with_company("11222333000181", "PADARIA CENTRAL LTDA", "ATIVA")
            .with_company("12ABC34501DE35", "OFICINA ALFANUMERICA LTDA", "BAIXADA")
    }

    /// The directory knows one address, and a city with a single code for all of it.
    fn known_postal_codes() -> InMemoryPostalCodeDirectory {
        InMemoryPostalCodeDirectory::default()
            .with_entry(
                "01310100",
                PostalCodeEntry {
                    street: Some("Avenida Paulista".into()),
                    district: Some("Bela Vista".into()),
                    city: "São Paulo".into(),
                    state: "SP".into(),
                },
            )
            .with_entry(
                "55555000",
                PostalCodeEntry {
                    street: None,
                    district: None,
                    city: "Cidade Pequena".into(),
                    state: "PE".into(),
                },
            )
    }

    fn app() -> Router {
        app_with_registry(known_companies())
    }

    /// No waiting between re-checks of a party, so tests can check one right after another.
    fn app_with_registry(registry: InMemoryCnpjRegistry) -> Router {
        app_with_parties(
            PartyService::new(InMemoryPartyRepository::default(), registry)
                .with_recheck_interval(Duration::ZERO),
        )
    }

    struct MemoryPorts;

    impl Ports for MemoryPorts {
        type Parties = InMemoryPartyRepository;
        type Registry = InMemoryCnpjRegistry;
        type Identity = InMemoryIdentityRepository;
        type Secrets = Argon2SecretService;
        type Products = InMemoryProductRepository;
        type CostCenters = InMemoryCostCenterRepository;
        type Categories = InMemoryCategoryRepository;
        type BankAccounts = InMemoryBankAccountRepository;
        type PaymentMethods = InMemoryPaymentMethodRepository;
        type PaymentTerms = InMemoryPaymentTermRepository;
        type PriceLists = InMemoryPriceListRepository;
        type Entries = InMemoryFinancialEntryRepository;
        type EntryReferences = TestReferences;
        type PeriodLocks = InMemoryPeriodLockRepository;
        type PackageSchedules = InMemoryPackageScheduleRepository;
        type ApiKeys = InMemoryApiKeyRepository;
        type BankConnections = InMemoryBankConnectionRepository;
        type Connectors = InMemoryConnectorRepository;
        type Aggregator = FakeBankAggregator;
        type WebhookSubscriptions = InMemoryWebhookRepository;
        type Jobs = InMemoryJobQueue;
        type SaleClaims = InMemorySaleClaimRepository;
        type FiscalSettings = InMemoryFiscalSettingsRepository;
        type GatewaySettings = InMemoryGatewaySettingsRepository;
        type Cipher = RingSecretCipher;
        type Gateway = FakeFiscalGateway;
        type FiscalDocuments = InMemoryFiscalDocumentRepository;
        type Attachments = InMemoryAttachmentRepository;
        type Blobs = InMemoryBlobStore;
        type Charges = InMemoryChargeRepository;
        type Provider = FakePaymentProvider;
        type CollectionSettings = InMemoryCollectionSettingsRepository;
        type PaymentEvents = InMemoryPaymentEventRepository;
        type PaymentLinks = InMemoryPaymentLinkRepository;
        type DunningPlans = InMemoryDunningPlanRepository;
        type Notifications = InMemoryNotificationRepository;
        type Messages = InMemoryMessageSender;
        type Transfers = InMemoryTransferRepository;
        type Openings = InMemoryOpeningRepository;
        type Recurrences = InMemoryRecurrenceRepository;
        type DocumentReferences = TestDocumentReferences;
        type Quotes = InMemoryQuoteRepository;
        type Sales = InMemorySaleRepository;
        type Purchases = InMemoryPurchaseOrderRepository;
        type StockMovements = InMemoryStockMovementRepository;
        type StockSettings = InMemoryStockSettingsRepository;
        type Counts = InMemoryInventoryCountRepository;
        type Contracts = InMemoryContractRepository;
        type CommissionRules = InMemoryCommissionRuleRepository;
        type Statements = InMemoryStatementLineRepository;
        type PostalCodes = InMemoryPostalCodeDirectory;
        type Invitations = InMemoryInvitationRepository;
        type Emails = InMemoryEmailSender;
        type DocumentMails = InMemoryDocumentMailRepository;
    }

    /// Like `MemoryPorts`, with the CNPJ and postal code lookups switched off.
    struct NoLookupPorts;

    impl Ports for NoLookupPorts {
        type Parties = InMemoryPartyRepository;
        type Registry = Option<InMemoryCnpjRegistry>;
        type Identity = InMemoryIdentityRepository;
        type Secrets = Argon2SecretService;
        type Products = InMemoryProductRepository;
        type CostCenters = InMemoryCostCenterRepository;
        type Categories = InMemoryCategoryRepository;
        type BankAccounts = InMemoryBankAccountRepository;
        type PaymentMethods = InMemoryPaymentMethodRepository;
        type PaymentTerms = InMemoryPaymentTermRepository;
        type PriceLists = InMemoryPriceListRepository;
        type Entries = InMemoryFinancialEntryRepository;
        type EntryReferences = TestReferences;
        type PeriodLocks = InMemoryPeriodLockRepository;
        type PackageSchedules = InMemoryPackageScheduleRepository;
        type ApiKeys = InMemoryApiKeyRepository;
        type BankConnections = InMemoryBankConnectionRepository;
        type Connectors = InMemoryConnectorRepository;
        type Aggregator = FakeBankAggregator;
        type WebhookSubscriptions = InMemoryWebhookRepository;
        type Jobs = InMemoryJobQueue;
        type SaleClaims = InMemorySaleClaimRepository;
        type FiscalSettings = InMemoryFiscalSettingsRepository;
        type GatewaySettings = InMemoryGatewaySettingsRepository;
        type Cipher = RingSecretCipher;
        type Gateway = FakeFiscalGateway;
        type FiscalDocuments = InMemoryFiscalDocumentRepository;
        type Attachments = InMemoryAttachmentRepository;
        type Blobs = InMemoryBlobStore;
        type Charges = InMemoryChargeRepository;
        type Provider = FakePaymentProvider;
        type CollectionSettings = InMemoryCollectionSettingsRepository;
        type PaymentEvents = InMemoryPaymentEventRepository;
        type PaymentLinks = InMemoryPaymentLinkRepository;
        type DunningPlans = InMemoryDunningPlanRepository;
        type Notifications = InMemoryNotificationRepository;
        type Messages = InMemoryMessageSender;
        type Transfers = InMemoryTransferRepository;
        type Openings = InMemoryOpeningRepository;
        type Recurrences = InMemoryRecurrenceRepository;
        type DocumentReferences = TestDocumentReferences;
        type Quotes = InMemoryQuoteRepository;
        type Sales = InMemorySaleRepository;
        type Purchases = InMemoryPurchaseOrderRepository;
        type StockMovements = InMemoryStockMovementRepository;
        type StockSettings = InMemoryStockSettingsRepository;
        type Counts = InMemoryInventoryCountRepository;
        type Contracts = InMemoryContractRepository;
        type CommissionRules = InMemoryCommissionRuleRepository;
        type Statements = InMemoryStatementLineRepository;
        type PostalCodes = Option<InMemoryPostalCodeDirectory>;
        type Invitations = InMemoryInvitationRepository;
        type Emails = InMemoryEmailSender;
        type DocumentMails = InMemoryDocumentMailRepository;
    }

    type TestReferences = RegisterReferences<
        InMemoryCategoryRepository,
        InMemoryCostCenterRepository,
        InMemoryPartyRepository,
        InMemoryBankAccountRepository,
        InMemoryPaymentMethodRepository,
        InMemoryPeriodLockRepository,
    >;

    thread_local! {
        /// The closed period every service built in a test sees: a test runs on one thread, so a test that
        /// builds an app and then closes a month through it, or through the handle `locks()` gives, is
        /// talking about one lock, without every helper below having to be told which.
        static LOCKS: std::cell::RefCell<InMemoryPeriodLockRepository> =
            std::cell::RefCell::new(InMemoryPeriodLockRepository::default());
    }

    thread_local! {
        /// The fiscal gateway the app of this test asks, so a test can have an invoice issued against the company.
        static GATEWAY: std::cell::RefCell<FakeFiscalGateway> =
            std::cell::RefCell::new(FakeFiscalGateway::default());
    }

    type TestGatewaySettings =
        GatewaySettingsService<InMemoryGatewaySettingsRepository, RingSecretCipher>;

    const SECRETS_KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn test_gateway_settings() -> TestGatewaySettings {
        GatewaySettingsService::new(
            InMemoryGatewaySettingsRepository::default(),
            RingSecretCipher::from_hex(SECRETS_KEY).unwrap(),
        )
    }

    fn gateway() -> FakeFiscalGateway {
        GATEWAY.with(|gateway| gateway.borrow().clone())
    }

    /// An invoice a supplier issued to the company, as the gateway would list it.
    fn received_invoice(number: u32, day: &str) -> conta_verde_domain::ReceivedInvoiceDraft {
        use conta_verde_domain::{
            AccessKey, Amount, CalendarDate, Manifestation, ReceivedInvoiceDraft, ReceivedSituation,
        };

        let day = CalendarDate::parse(day).unwrap();
        ReceivedInvoiceDraft {
            access_key: AccessKey::from_parts(
                35,
                day,
                "12345678000195",
                55,
                1,
                number,
                1,
                10_000_000 + number,
            )
            .unwrap(),
            issuer_name: format!("Fornecedor {number} Ltda"),
            issuer_document: "12345678000195".into(),
            total: Amount::parse("150.50").unwrap(),
            issued_on: day,
            situation: ReceivedSituation::Authorized,
            manifestation: Manifestation::None,
            version: 0,
        }
    }

    const FISCAL_HOOK_SECRET: &str = "a-fiscal-webhook-secret-of-32-chars-or-more";

    fn locks() -> InMemoryPeriodLockRepository {
        LOCKS.with(|locks| locks.borrow().clone())
    }

    type TestEntries = FinancialEntryService<InMemoryFinancialEntryRepository, TestReferences>;

    /// Entries over registers that the app's own services do not share: for the tests that never
    /// register an entry. `entries_over` is the one that does.
    fn standalone_entries() -> TestEntries {
        entries_over(
            InMemoryFinancialEntryRepository::default(),
            &InMemoryCategoryRepository::default(),
            &InMemoryCostCenterRepository::default(),
            &InMemoryPartyRepository::default(),
            &InMemoryBankAccountRepository::default(),
            &InMemoryPaymentMethodRepository::default(),
        )
    }

    /// Entries that read the registers a test keeps a handle to, so what it registers there can be
    /// pointed at.
    fn entries_over(
        entries: InMemoryFinancialEntryRepository,
        categories: &InMemoryCategoryRepository,
        cost_centers: &InMemoryCostCenterRepository,
        parties: &InMemoryPartyRepository,
        bank_accounts: &InMemoryBankAccountRepository,
        payment_methods: &InMemoryPaymentMethodRepository,
    ) -> TestEntries {
        FinancialEntryService::new(
            entries,
            references_over(
                categories,
                cost_centers,
                parties,
                bank_accounts,
                payment_methods,
            ),
        )
    }

    fn references_over(
        categories: &InMemoryCategoryRepository,
        cost_centers: &InMemoryCostCenterRepository,
        parties: &InMemoryPartyRepository,
        bank_accounts: &InMemoryBankAccountRepository,
        payment_methods: &InMemoryPaymentMethodRepository,
    ) -> TestReferences {
        RegisterReferences::new(
            categories.clone(),
            cost_centers.clone(),
            parties.clone(),
            bank_accounts.clone(),
            payment_methods.clone(),
            locks(),
        )
    }

    type TestFiscalDocuments = FiscalDocumentService<
        InMemoryFiscalDocumentRepository,
        InMemorySaleRepository,
        InMemoryPartyRepository,
        InMemoryProductRepository,
        InMemoryFiscalSettingsRepository,
        FakeFiscalGateway,
        InMemoryBlobStore,
        TestReferences,
    >;

    /// Fiscal documents over the registers a test keeps a handle to, so a sale it saves there can be invoiced.
    fn fiscal_documents_over(
        sales: &InMemorySaleRepository,
        parties: &InMemoryPartyRepository,
        products: &InMemoryProductRepository,
        settings: &InMemoryFiscalSettingsRepository,
        gateway: &FakeFiscalGateway,
    ) -> TestFiscalDocuments {
        FiscalDocumentService::new(
            InMemoryFiscalDocumentRepository::default(),
            sales.clone(),
            parties.clone(),
            products.clone(),
            settings.clone(),
            gateway.clone(),
            InMemoryBlobStore::default(),
            references_over(
                &InMemoryCategoryRepository::default(),
                &InMemoryCostCenterRepository::default(),
                parties,
                &InMemoryBankAccountRepository::default(),
                &InMemoryPaymentMethodRepository::default(),
            ),
        )
    }

    fn standalone_fiscal_documents() -> TestFiscalDocuments {
        fiscal_documents_over(
            &InMemorySaleRepository::default(),
            &InMemoryPartyRepository::default(),
            &InMemoryProductRepository::default(),
            &InMemoryFiscalSettingsRepository::default(),
            &FakeFiscalGateway::default(),
        )
    }

    type TestAttachments = AttachmentService<
        InMemoryAttachmentRepository,
        InMemoryBlobStore,
        InMemoryFinancialEntryRepository,
    >;

    /// Attachments over an entry store that the app's own services do not share: for the tests that
    /// never attach a file. `entry_app` is the one that does.
    fn standalone_attachments() -> TestAttachments {
        AttachmentService::new(
            InMemoryAttachmentRepository::default(),
            InMemoryBlobStore::default(),
            InMemoryFinancialEntryRepository::default(),
        )
    }

    type TestExports = ExportsOf<MemoryPorts>;

    /// Exports over stores that the app's own services do not share: for the tests that never export.
    fn standalone_exports() -> TestExports {
        AccountingExportService::new(
            InMemoryFinancialEntryRepository::default(),
            InMemoryTransferRepository::default(),
            InMemoryAttachmentRepository::default(),
            InMemoryBlobStore::default(),
            references_over(
                &InMemoryCategoryRepository::default(),
                &InMemoryCostCenterRepository::default(),
                &InMemoryPartyRepository::default(),
                &InMemoryBankAccountRepository::default(),
                &InMemoryPaymentMethodRepository::default(),
            ),
        )
    }

    /// The schedule over stores that the app's own services do not share: for the tests that never use it.
    type TestHooks = WebhookService<
        InMemoryWebhookRepository,
        RingSecretCipher,
        InMemoryJobQueue,
        Argon2SecretService,
    >;

    /// Webhooks that go to a listener on this machine: the opt-in a test makes.
    fn standalone_hooks() -> Arc<TestHooks> {
        Arc::new(WebhookService::new(
            InMemoryWebhookRepository::default(),
            RingSecretCipher::from_hex(&"33".repeat(32)).unwrap(),
            InMemoryJobQueue::default(),
            Argon2SecretService::new().unwrap(),
            true,
        ))
    }

    thread_local! {
        /// The webhooks of the app of this test, so a test can run the worker that sends them.
        static HOOKS: std::cell::RefCell<Option<Arc<TestHooks>>> = const { std::cell::RefCell::new(None) };
    }

    type TestBankSync = BankSyncOf<MemoryPorts>;

    /// Bank connections over an aggregator and statements of their own.
    fn standalone_bank_sync() -> TestBankSync {
        Arc::new(BankSyncService::new(
            InMemoryBankConnectionRepository::default(),
            FakeBankAggregator::default(),
            standalone_reconciliation(),
            InMemoryJobQueue::default(),
        ))
    }

    thread_local! {
        /// The bank aggregator of the app of this test, so a test can tell it what the bank holds.
        static AGGREGATOR: std::cell::RefCell<FakeBankAggregator> =
            std::cell::RefCell::new(FakeBankAggregator::default());
    }

    fn standalone_api_keys() -> Arc<ApiKeyService<InMemoryApiKeyRepository, Argon2SecretService>> {
        Arc::new(ApiKeyService::new(
            InMemoryApiKeyRepository::default(),
            Argon2SecretService::new().unwrap(),
        ))
    }

    fn standalone_packages() -> PackagesOf<MemoryPorts> {
        PackageScheduleService::new(
            InMemoryPackageScheduleRepository::default(),
            InMemoryIdentityRepository::default(),
            InMemoryEmailSender::default(),
            "http://localhost:5173",
        )
    }

    type TestCharges = ChargesOf<MemoryPorts>;

    /// Charges over stores that the app's own services do not share: for the tests that never charge.
    fn standalone_charges() -> TestCharges {
        ChargeService::new(
            InMemoryChargeRepository::default(),
            InMemoryFinancialEntryRepository::default(),
            InMemoryPartyRepository::default(),
            FakePaymentProvider::default(),
            InMemoryCollectionSettingsRepository::default(),
        )
    }

    fn standalone_webhooks() -> WebhooksOf<MemoryPorts> {
        webhooks_over(
            InMemoryChargeRepository::default(),
            InMemoryCollectionSettingsRepository::default(),
            InMemoryPaymentEventRepository::default(),
            standalone_entries(),
        )
    }

    fn webhooks_over(
        charges: InMemoryChargeRepository,
        settings: InMemoryCollectionSettingsRepository,
        events: InMemoryPaymentEventRepository,
        entries: TestEntries,
    ) -> WebhooksOf<MemoryPorts> {
        PaymentWebhookService::new(
            FakePaymentProvider::default(),
            charges,
            events,
            settings,
            entries,
        )
    }

    fn payment_pages_over(
        charges: InMemoryChargeRepository,
        entries: InMemoryFinancialEntryRepository,
    ) -> PaymentPagesOf<MemoryPorts> {
        PaymentPageService::new(
            InMemoryPaymentLinkRepository::default(),
            charges,
            entries,
            Argon2SecretService::new().unwrap(),
            "https://app.example",
        )
    }

    fn dunning_over(
        entries: InMemoryFinancialEntryRepository,
        charges: InMemoryChargeRepository,
        parties: InMemoryPartyRepository,
    ) -> DunningOf<MemoryPorts> {
        DunningService::new(
            InMemoryDunningPlanRepository::default(),
            entries.clone(),
            parties,
            payment_pages_over(charges, entries),
            InMemoryNotificationRepository::default(),
            InMemoryIdentityRepository::default(),
            InMemoryEmailSender::default(),
            InMemoryMessageSender::default(),
        )
    }

    fn standalone_dunning() -> DunningOf<MemoryPorts> {
        dunning_over(
            InMemoryFinancialEntryRepository::default(),
            InMemoryChargeRepository::default(),
            InMemoryPartyRepository::default(),
        )
    }

    fn standalone_payment_pages() -> PaymentPagesOf<MemoryPorts> {
        payment_pages_over(
            InMemoryChargeRepository::default(),
            InMemoryFinancialEntryRepository::default(),
        )
    }

    type TestCollection = CollectionService<InMemoryCollectionSettingsRepository, TestReferences>;

    fn standalone_collection() -> TestCollection {
        CollectionService::new(
            InMemoryCollectionSettingsRepository::default(),
            references_over(
                &InMemoryCategoryRepository::default(),
                &InMemoryCostCenterRepository::default(),
                &InMemoryPartyRepository::default(),
                &InMemoryBankAccountRepository::default(),
                &InMemoryPaymentMethodRepository::default(),
            ),
        )
    }

    type TestTransfers = TransferService<InMemoryTransferRepository, TestReferences>;

    type TestBalances = BalanceService<
        InMemoryBankAccountRepository,
        InMemoryFinancialEntryRepository,
        InMemoryTransferRepository,
        InMemoryOpeningRepository,
        InMemoryPeriodLockRepository,
    >;

    /// Transfers and balances over registers that the app's own services do not share.
    fn standalone_transfers() -> TestTransfers {
        TransferService::new(
            InMemoryTransferRepository::default(),
            references_over(
                &InMemoryCategoryRepository::default(),
                &InMemoryCostCenterRepository::default(),
                &InMemoryPartyRepository::default(),
                &InMemoryBankAccountRepository::default(),
                &InMemoryPaymentMethodRepository::default(),
            ),
        )
    }

    type TestReconciliation = ReconciliationService<
        InMemoryStatementLineRepository,
        InMemoryFinancialEntryRepository,
        InMemoryTransferRepository,
        TestReferences,
    >;

    /// Reconciliation over registers that the app's own services do not share.
    fn standalone_reconciliation() -> TestReconciliation {
        let registers = (
            InMemoryCategoryRepository::default(),
            InMemoryCostCenterRepository::default(),
            InMemoryPartyRepository::default(),
            InMemoryBankAccountRepository::default(),
            InMemoryPaymentMethodRepository::default(),
        );
        reconciliation_over(
            InMemoryFinancialEntryRepository::default(),
            InMemoryTransferRepository::default(),
            &registers.0,
            &registers.1,
            &registers.2,
            &registers.3,
            &registers.4,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn reconciliation_over(
        entries: InMemoryFinancialEntryRepository,
        transfers: InMemoryTransferRepository,
        categories: &InMemoryCategoryRepository,
        cost_centers: &InMemoryCostCenterRepository,
        parties: &InMemoryPartyRepository,
        bank_accounts: &InMemoryBankAccountRepository,
        payment_methods: &InMemoryPaymentMethodRepository,
    ) -> TestReconciliation {
        reconciliation_with(
            InMemoryStatementLineRepository::default(),
            entries,
            transfers,
            categories,
            cost_centers,
            parties,
            bank_accounts,
            payment_methods,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn reconciliation_with(
        statements: InMemoryStatementLineRepository,
        entries: InMemoryFinancialEntryRepository,
        transfers: InMemoryTransferRepository,
        categories: &InMemoryCategoryRepository,
        cost_centers: &InMemoryCostCenterRepository,
        parties: &InMemoryPartyRepository,
        bank_accounts: &InMemoryBankAccountRepository,
        payment_methods: &InMemoryPaymentMethodRepository,
    ) -> TestReconciliation {
        let references = || {
            references_over(
                categories,
                cost_centers,
                parties,
                bank_accounts,
                payment_methods,
            )
        };
        ReconciliationService::new(
            statements,
            FinancialEntryService::new(entries, references()),
            TransferService::new(transfers, references()),
            references(),
        )
    }

    type TestRecurrences = RecurrenceService<
        InMemoryRecurrenceRepository,
        InMemoryFinancialEntryRepository,
        TestReferences,
    >;

    /// Recurrences over registers that the app's own services do not share.
    fn standalone_recurrences() -> TestRecurrences {
        let registers = (
            InMemoryCategoryRepository::default(),
            InMemoryCostCenterRepository::default(),
            InMemoryPartyRepository::default(),
            InMemoryBankAccountRepository::default(),
            InMemoryPaymentMethodRepository::default(),
        );
        recurrences_over(
            InMemoryFinancialEntryRepository::default(),
            &registers.0,
            &registers.1,
            &registers.2,
            &registers.3,
            &registers.4,
        )
    }

    fn recurrences_over(
        entries: InMemoryFinancialEntryRepository,
        categories: &InMemoryCategoryRepository,
        cost_centers: &InMemoryCostCenterRepository,
        parties: &InMemoryPartyRepository,
        bank_accounts: &InMemoryBankAccountRepository,
        payment_methods: &InMemoryPaymentMethodRepository,
    ) -> TestRecurrences {
        let references = || {
            references_over(
                categories,
                cost_centers,
                parties,
                bank_accounts,
                payment_methods,
            )
        };
        RecurrenceService::new(
            InMemoryRecurrenceRepository::default(),
            FinancialEntryService::new(entries, references()),
            references(),
        )
    }

    fn standalone_balances() -> TestBalances {
        BalanceService::new(
            InMemoryBankAccountRepository::default(),
            InMemoryFinancialEntryRepository::default(),
            InMemoryTransferRepository::default(),
            InMemoryOpeningRepository::default(),
        )
        .with_period_lock(locks())
    }

    type TestDocumentReferences = RegisterDocumentReferences<
        InMemoryPartyRepository,
        InMemoryProductRepository,
        InMemoryPaymentTermRepository,
    >;

    type TestQuotes = QuoteService<
        InMemoryQuoteRepository,
        TestDocumentReferences,
        InMemoryEmailSender,
        Argon2SecretService,
    >;

    /// Quotes over the registers a test keeps a handle to, so what it registers there can be pointed at.
    fn quotes_over(
        store: InMemoryQuoteRepository,
        parties: &InMemoryPartyRepository,
        products: &InMemoryProductRepository,
        payment_terms: &InMemoryPaymentTermRepository,
    ) -> TestQuotes {
        QuoteService::new(
            store,
            RegisterDocumentReferences::new(
                parties.clone(),
                products.clone(),
                payment_terms.clone(),
            ),
            InMemoryEmailSender::default(),
            Argon2SecretService::new().unwrap(),
            "https://erp.example",
        )
    }

    /// Quotes over registers that the app's own services do not share.
    fn standalone_quotes() -> TestQuotes {
        quotes_over(
            InMemoryQuoteRepository::default(),
            &InMemoryPartyRepository::default(),
            &InMemoryProductRepository::default(),
            &InMemoryPaymentTermRepository::default(),
        )
    }

    type TestSales = SaleService<
        InMemorySaleRepository,
        InMemoryQuoteRepository,
        InMemoryFinancialEntryRepository,
        TestReferences,
        TestDocumentReferences,
        TestStock,
    >;

    type TestStock = StockService<
        InMemoryStockMovementRepository,
        InMemoryProductRepository,
        InMemoryStockSettingsRepository,
        InMemoryPeriodLockRepository,
    >;

    /// Sales over the registers, the quotes and the entries a test keeps a handle to, so that a quote can be
    /// turned into a sale and the sale's receivable shows in the entries and the cash flow.
    #[allow(clippy::too_many_arguments)]
    fn sales_over(
        store: InMemorySaleRepository,
        quotes: InMemoryQuoteRepository,
        entries: InMemoryFinancialEntryRepository,
        categories: &InMemoryCategoryRepository,
        cost_centers: &InMemoryCostCenterRepository,
        parties: &InMemoryPartyRepository,
        bank_accounts: &InMemoryBankAccountRepository,
        payment_methods: &InMemoryPaymentMethodRepository,
        products: &InMemoryProductRepository,
        payment_terms: &InMemoryPaymentTermRepository,
        ledger: InMemoryStockMovementRepository,
        settings: InMemoryStockSettingsRepository,
    ) -> TestSales {
        let references = || {
            references_over(
                categories,
                cost_centers,
                parties,
                bank_accounts,
                payment_methods,
            )
        };
        SaleService::new(
            store,
            quotes,
            FinancialEntryService::new(entries, references()),
            references(),
            RegisterDocumentReferences::new(
                parties.clone(),
                products.clone(),
                payment_terms.clone(),
            ),
            StockService::new(ledger, products.clone(), settings).with_period_lock(locks()),
        )
    }

    /// Sales over registers that the app's own services do not share.
    fn standalone_sales() -> TestSales {
        sales_over(
            InMemorySaleRepository::default(),
            InMemoryQuoteRepository::default(),
            InMemoryFinancialEntryRepository::default(),
            &InMemoryCategoryRepository::default(),
            &InMemoryCostCenterRepository::default(),
            &InMemoryPartyRepository::default(),
            &InMemoryBankAccountRepository::default(),
            &InMemoryPaymentMethodRepository::default(),
            &InMemoryProductRepository::default(),
            &InMemoryPaymentTermRepository::default(),
            InMemoryStockMovementRepository::default(),
            InMemoryStockSettingsRepository::default(),
        )
    }

    type TestPurchases = PurchaseService<
        InMemoryPurchaseOrderRepository,
        InMemoryFinancialEntryRepository,
        TestReferences,
        TestDocumentReferences,
        TestStock,
    >;

    /// Purchase orders over the registers and the entries a test keeps a handle to, so that what a receipt
    /// makes shows in the entries.
    #[allow(clippy::too_many_arguments)]
    fn purchases_over(
        store: InMemoryPurchaseOrderRepository,
        entries: InMemoryFinancialEntryRepository,
        categories: &InMemoryCategoryRepository,
        cost_centers: &InMemoryCostCenterRepository,
        parties: &InMemoryPartyRepository,
        bank_accounts: &InMemoryBankAccountRepository,
        payment_methods: &InMemoryPaymentMethodRepository,
        products: &InMemoryProductRepository,
        payment_terms: &InMemoryPaymentTermRepository,
        ledger: InMemoryStockMovementRepository,
        settings: InMemoryStockSettingsRepository,
    ) -> TestPurchases {
        let references = || {
            references_over(
                categories,
                cost_centers,
                parties,
                bank_accounts,
                payment_methods,
            )
        };
        PurchaseService::new(
            store,
            FinancialEntryService::new(entries, references()),
            references(),
            RegisterDocumentReferences::new(
                parties.clone(),
                products.clone(),
                payment_terms.clone(),
            ),
            StockService::new(ledger, products.clone(), settings).with_period_lock(locks()),
        )
    }

    /// Purchase orders over registers that the app's own services do not share.
    fn standalone_purchases() -> TestPurchases {
        purchases_over(
            InMemoryPurchaseOrderRepository::default(),
            InMemoryFinancialEntryRepository::default(),
            &InMemoryCategoryRepository::default(),
            &InMemoryCostCenterRepository::default(),
            &InMemoryPartyRepository::default(),
            &InMemoryBankAccountRepository::default(),
            &InMemoryPaymentMethodRepository::default(),
            &InMemoryProductRepository::default(),
            &InMemoryPaymentTermRepository::default(),
            InMemoryStockMovementRepository::default(),
            InMemoryStockSettingsRepository::default(),
        )
    }

    type TestCounts = InventoryCountService<InMemoryInventoryCountRepository, TestStock>;

    /// Inventory counts over the stock a test keeps a handle to.
    fn counts_over(stock: TestStock) -> TestCounts {
        InventoryCountService::new(InMemoryInventoryCountRepository::default(), stock)
    }

    /// Inventory counts over a stock that the app's own services do not share.
    fn standalone_counts() -> TestCounts {
        counts_over(standalone_stock())
    }

    /// The stock over registers that the app's own services do not share.
    fn standalone_stock() -> TestStock {
        StockService::new(
            InMemoryStockMovementRepository::default(),
            InMemoryProductRepository::default(),
            InMemoryStockSettingsRepository::default(),
        )
        .with_period_lock(locks())
    }

    type TestPriceLists = PriceListService<InMemoryPriceListRepository, TestDocumentReferences>;

    /// Price lists over the registers a test keeps a handle to.
    fn price_lists_over(
        parties: &InMemoryPartyRepository,
        products: &InMemoryProductRepository,
        payment_terms: &InMemoryPaymentTermRepository,
    ) -> TestPriceLists {
        PriceListService::new(
            InMemoryPriceListRepository::default(),
            RegisterDocumentReferences::new(
                parties.clone(),
                products.clone(),
                payment_terms.clone(),
            ),
        )
    }

    /// Price lists over registers that the app's own do not share.
    fn standalone_price_lists() -> TestPriceLists {
        price_lists_over(
            &InMemoryPartyRepository::default(),
            &InMemoryProductRepository::default(),
            &InMemoryPaymentTermRepository::default(),
        )
    }

    type TestCommissions = CommissionService<
        InMemoryCommissionRuleRepository,
        InMemorySaleRepository,
        TestDocumentReferences,
    >;

    /// Commissions over the sales store and the registers a test keeps a handle to.
    fn commissions_over(
        sales: InMemorySaleRepository,
        parties: &InMemoryPartyRepository,
        products: &InMemoryProductRepository,
        payment_terms: &InMemoryPaymentTermRepository,
    ) -> TestCommissions {
        CommissionService::new(
            InMemoryCommissionRuleRepository::default(),
            sales,
            RegisterDocumentReferences::new(
                parties.clone(),
                products.clone(),
                payment_terms.clone(),
            ),
        )
    }

    /// Commissions over a store that the app's own sales do not share.
    fn standalone_commissions() -> TestCommissions {
        commissions_over(
            InMemorySaleRepository::default(),
            &InMemoryPartyRepository::default(),
            &InMemoryProductRepository::default(),
            &InMemoryPaymentTermRepository::default(),
        )
    }

    type TestDocuments = DocumentMailService<
        InMemoryPurchaseOrderRepository,
        InMemoryFinancialEntryRepository,
        InMemoryPartyRepository,
        InMemoryEmailSender,
        InMemoryDocumentMailRepository,
    >;

    type TestMembers = MemberService<
        InMemoryInvitationRepository,
        InMemoryIdentityRepository,
        InMemoryEmailSender,
        Argon2SecretService,
    >;

    /// The members service over the identities a test keeps a handle to, so what is accepted shows up
    /// in what the app knows, and the e-mails it sends can be read.
    fn members_over(
        identities: &InMemoryIdentityRepository,
        emails: &InMemoryEmailSender,
    ) -> TestMembers {
        MemberService::new(
            InMemoryInvitationRepository::new(identities.clone()),
            identities.clone(),
            emails.clone(),
            Argon2SecretService::new().unwrap(),
            Duration::from_secs(7 * 86_400),
            "https://erp.example",
        )
    }

    /// One that shares nothing with the app's identities: for the tests that never invite.
    fn standalone_documents() -> TestDocuments {
        DocumentMailService::new(
            InMemoryPurchaseOrderRepository::default(),
            InMemoryFinancialEntryRepository::default(),
            InMemoryPartyRepository::default(),
            InMemoryEmailSender::default(),
            InMemoryDocumentMailRepository::default(),
        )
    }

    fn standalone_members() -> TestMembers {
        members_over(
            &InMemoryIdentityRepository::default(),
            &InMemoryEmailSender::default(),
        )
    }

    fn auth_service() -> AuthService<InMemoryIdentityRepository, Argon2SecretService> {
        AuthService::new(
            InMemoryIdentityRepository::default(),
            Argon2SecretService::new().unwrap(),
            Duration::from_secs(3600),
        )
    }

    fn app_with_parties(
        parties: PartyService<InMemoryPartyRepository, InMemoryCnpjRegistry>,
    ) -> Router {
        app_with(parties, known_postal_codes())
    }

    fn app_with(
        parties: PartyService<InMemoryPartyRepository, InMemoryCnpjRegistry>,
        postal_codes: InMemoryPostalCodeDirectory,
    ) -> Router {
        app_with_auth(parties, postal_codes, auth_service(), standalone_members())
    }

    fn app_with_auth(
        parties: PartyService<InMemoryPartyRepository, InMemoryCnpjRegistry>,
        postal_codes: InMemoryPostalCodeDirectory,
        auth: AuthService<InMemoryIdentityRepository, Argon2SecretService>,
        members: TestMembers,
    ) -> Router {
        app_with_fiscal(
            parties,
            postal_codes,
            auth,
            members,
            standalone_fiscal_documents(),
            InMemoryFiscalSettingsRepository::default(),
        )
    }

    /// The app over fiscal documents and settings a test keeps handles to, so the sale it saves is the one invoiced.
    fn app_with_fiscal(
        parties: PartyService<InMemoryPartyRepository, InMemoryCnpjRegistry>,
        postal_codes: InMemoryPostalCodeDirectory,
        auth: AuthService<InMemoryIdentityRepository, Argon2SecretService>,
        members: TestMembers,
        fiscal_documents: TestFiscalDocuments,
        fiscal_settings: InMemoryFiscalSettingsRepository,
    ) -> Router {
        router::<MemoryPorts>(
            Services {
                bank_sync: standalone_bank_sync(),
                webhook_subscriptions: standalone_hooks(),
                api_keys: standalone_api_keys(),
                connectors: InMemoryConnectorRepository::default(),
                sale_claims: InMemorySaleClaimRepository::default(),
                parties,
                products: ProductService::new(InMemoryProductRepository::default()),
                cost_centers: CostCenterService::new(InMemoryCostCenterRepository::default()),
                categories: CategoryService::new(InMemoryCategoryRepository::default()),
                bank_accounts: BankAccountService::new(InMemoryBankAccountRepository::default()),
                payment_methods: PaymentMethodService::new(
                    InMemoryPaymentMethodRepository::default(),
                ),
                payment_terms: PaymentTermService::new(InMemoryPaymentTermRepository::default()),
                entries: standalone_entries(),
                attachments: standalone_attachments(),
                charges: standalone_charges(),
                webhooks: standalone_webhooks(),
                payment_pages: standalone_payment_pages(),
                dunning: standalone_dunning(),
                collection: standalone_collection(),
                transfers: standalone_transfers(),
                recurrences: standalone_recurrences(),
                quotes: standalone_quotes(),
                sales: standalone_sales(),
                purchases: standalone_purchases(),
                stock: standalone_stock(),
                counts: standalone_counts(),
                contracts: ContractService::new(
                    InMemoryContractRepository::default(),
                    standalone_sales(),
                ),
                commissions: standalone_commissions(),
                price_lists: standalone_price_lists(),
                sales_reports: SalesReportService::new(InMemorySaleRepository::default()),
                reconciliation: standalone_reconciliation(),
                balances: standalone_balances(),
                period_locks: PeriodLockService::new(locks()),
                exports: standalone_exports(),
                packages: standalone_packages(),
                fiscal_documents,
                fiscal_settings: FiscalSettingsService::new(fiscal_settings),
                gateway_settings: test_gateway_settings(),
                dre: DreService::new(
                    InMemoryFinancialEntryRepository::default(),
                    InMemoryCategoryRepository::default(),
                ),
                postal_codes: PostalCodeService::new(postal_codes),
                members,
                documents: standalone_documents(),
                auth,
            },
            ApiConfig::default(),
        )
    }

    struct Reply {
        status: StatusCode,
        set_cookie: Option<String>,
        body: Value,
    }

    async fn send(app: &Router, request: Request<Body>) -> Reply {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .map(|value| value.to_str().unwrap().to_owned());
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        Reply {
            status,
            set_cookie,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }

    fn json_request(method: &str, uri: &str, cookie: Option<&str>, body: Value) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        builder.body(Body::from(body.to_string())).unwrap()
    }

    fn get_request(uri: &str, cookie: Option<&str>) -> Request<Body> {
        let mut builder = Request::get(uri);
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        builder.body(Body::empty()).unwrap()
    }

    /// Registers a user and returns the cookie header to send back plus their company id.
    async fn sign_up(app: &Router, email: &str) -> (String, Uuid) {
        let reply = send(
            app,
            json_request(
                "POST",
                "/auth/register",
                None,
                json!({
                    "name": "Ana",
                    "email": email,
                    "password": "correct horse",
                    "company_name": "Padaria"
                }),
            ),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CREATED);
        let cookie = reply.set_cookie.unwrap();
        let pair = cookie.split(';').next().unwrap().to_owned();
        let company_id = reply.body["companies"][0]["id"].as_str().unwrap();
        (pair, company_id.parse().unwrap())
    }

    fn party_body() -> Value {
        json!({
            "name": "Padaria Central",
            "document": "11.222.333/0001-81",
            "is_customer": true,
            "is_supplier": false
        })
    }

    fn parties_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/parties")
    }

    #[tokio::test]
    async fn health_reports_ok() {
        let reply = send(&app(), get_request("/health", None)).await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(reply.body["status"], "ok");
    }

    #[tokio::test]
    async fn registration_sets_a_hardened_session_cookie() {
        let app = app();
        let reply = send(
            &app,
            json_request(
                "POST",
                "/auth/register",
                None,
                json!({
                    "name": "Ana",
                    "email": "ana@example.com",
                    "password": "correct horse",
                    "company_name": "Padaria"
                }),
            ),
        )
        .await;
        let cookie = reply.set_cookie.unwrap();
        assert!(cookie.starts_with("cv_session="));
        assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Lax"));
        assert!(!reply.body.to_string().contains("password"));
    }

    #[tokio::test]
    async fn me_requires_a_session_and_reports_companies() {
        let app = app();
        let anonymous = send(&app, get_request("/auth/me", None)).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let reply = send(&app, get_request("/auth/me", Some(&cookie))).await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(reply.body["user"]["email"], "ana@example.com");
        assert_eq!(
            reply.body["companies"][0]["id"],
            company_id.to_string().as_str()
        );
    }

    #[tokio::test]
    async fn login_checks_credentials_and_logout_ends_the_session() {
        let app = app();
        sign_up(&app, "ana@example.com").await;

        let wrong = send(
            &app,
            json_request(
                "POST",
                "/auth/login",
                None,
                json!({ "email": "ana@example.com", "password": "nope nope" }),
            ),
        )
        .await;
        assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
        assert!(wrong.set_cookie.is_none());

        let ok = send(
            &app,
            json_request(
                "POST",
                "/auth/login",
                None,
                json!({ "email": "ana@example.com", "password": "correct horse" }),
            ),
        )
        .await;
        assert_eq!(ok.status, StatusCode::OK);
        let cookie = ok.set_cookie.unwrap().split(';').next().unwrap().to_owned();

        let logout = send(
            &app,
            json_request("POST", "/auth/logout", Some(&cookie), Value::Null),
        )
        .await;
        assert_eq!(logout.status, StatusCode::NO_CONTENT);
        assert!(logout.set_cookie.unwrap().contains("Max-Age=0"));

        let after = send(&app, get_request("/auth/me", Some(&cookie))).await;
        assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn duplicate_email_conflicts_and_weak_password_is_unprocessable() {
        let app = app();
        sign_up(&app, "ana@example.com").await;

        let register = |email: &str, password: &str| {
            json_request(
                "POST",
                "/auth/register",
                None,
                json!({
                    "name": "Outra",
                    "email": email,
                    "password": password,
                    "company_name": "Outra"
                }),
            )
        };
        let duplicate = send(&app, register("ANA@example.com", "correct horse")).await;
        assert_eq!(duplicate.status, StatusCode::CONFLICT);
        let weak = send(&app, register("new@example.com", "short")).await;
        assert_eq!(weak.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn parties_require_a_session() {
        let app = app();
        let (_, company_id) = sign_up(&app, "ana@example.com").await;
        let reply = send(&app, get_request(&parties_uri(company_id), None)).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn anonymous_callers_are_rejected_before_the_body_is_parsed() {
        let app = app();
        let (_, company_id) = sign_up(&app, "ana@example.com").await;
        let malformed = json!({ "unexpected": true });
        let reply = send(
            &app,
            json_request("POST", &parties_uri(company_id), None, malformed),
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn registers_then_lists_a_party() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = parties_uri(company_id);

        let created = send(
            &app,
            json_request("POST", &uri, Some(&cookie), party_body()),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED);
        assert_eq!(created.body["document"], "11222333000181");

        let listed = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(listed.status, StatusCode::OK);
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
        assert!(listed.body["next_cursor"].is_null());
    }

    fn address_body() -> Value {
        json!({
            "postal_code": "01310-100",
            "street": "Avenida Paulista",
            "number": "1000",
            "city": "São Paulo",
            "state": "sp"
        })
    }

    #[tokio::test]
    async fn registers_and_changes_parties_with_roles_and_an_address() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = parties_uri(company_id);

        let mut body = party_body();
        body["is_carrier"] = json!(true);
        body["address"] = address_body();
        body["tags"] = json!(["VIP", "atacado", "vip"]);
        body["contacts"] = json!([
            { "name": " Bruno ", "job_title": "Compras", "email": "Bruno@Padaria.example", "phone": "(81) 99999-0000" },
            { "name": "Carla" }
        ]);
        let created = send(&app, json_request("POST", &uri, Some(&cookie), body)).await;
        assert_eq!(created.status, StatusCode::CREATED);
        assert_eq!(created.body["is_carrier"], true);
        assert_eq!(created.body["is_seller"], false);
        assert_eq!(created.body["tags"], json!(["atacado", "vip"]));
        assert_eq!(
            created.body["contacts"],
            json!([
                { "name": "Bruno", "job_title": "Compras", "email": "bruno@padaria.example", "phone": "(81) 99999-0000" },
                { "name": "Carla", "job_title": null, "email": null, "phone": null }
            ])
        );
        assert_eq!(
            created.body["address"],
            json!({
                "postal_code": "01310100",
                "street": "Avenida Paulista",
                "number": "1000",
                "complement": null,
                "district": null,
                "city": "São Paulo",
                "state": "SP"
            })
        );
        let id = created.body["id"].as_str().unwrap().to_owned();
        let one = format!("{uri}/{id}");

        let tagged = send(&app, get_request(&format!("{uri}?tag=VIP"), Some(&cookie))).await;
        assert_eq!(tagged.body["items"].as_array().unwrap().len(), 1);

        // A change replaces every field: no address, a seller instead of a carrier, one tag fewer
        // and a contact fewer.
        let mut replacement = party_body();
        replacement["name"] = json!("Padaria do Centro");
        replacement["is_carrier"] = json!(false);
        replacement["is_seller"] = json!(true);
        replacement["tags"] = json!(["vip"]);
        replacement["contacts"] = json!([{ "name": "Carla" }]);
        let put = json_request("PUT", &one, Some(&cookie), replacement.clone());
        let changed = send(&app, put).await;
        assert_eq!(changed.status, StatusCode::OK);
        assert_eq!(changed.body["name"], "Padaria do Centro");
        assert_eq!(changed.body["is_seller"], true);
        assert!(changed.body["address"].is_null());
        assert_eq!(changed.body["tags"], json!(["vip"]));
        assert_eq!(changed.body["contacts"].as_array().unwrap().len(), 1);
        let gone = send(
            &app,
            get_request(&format!("{uri}?tag=atacado"), Some(&cookie)),
        )
        .await;
        assert!(gone.body["items"].as_array().unwrap().is_empty());
        // The document did not change, so what the registry said stays.
        assert_eq!(changed.body["document_check"]["status"], "verified");

        let sellers = send(
            &app,
            get_request(&format!("{uri}?kind=seller"), Some(&cookie)),
        )
        .await;
        assert_eq!(sellers.body["items"].as_array().unwrap().len(), 1);
        let carriers = send(
            &app,
            get_request(&format!("{uri}?kind=carrier"), Some(&cookie)),
        )
        .await;
        assert!(carriers.body["items"].as_array().unwrap().is_empty());
        let unknown = send(
            &app,
            get_request(&format!("{uri}?kind=driver"), Some(&cookie)),
        )
        .await;
        assert_eq!(unknown.status, StatusCode::BAD_REQUEST);

        let history = send(&app, get_request(&format!("{one}/history"), Some(&cookie))).await;
        let entries = history.body.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        let fields: Vec<&str> = entries[1]["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|change| change["field"].as_str().unwrap())
            .collect();
        assert_eq!(
            fields,
            [
                "name",
                "is_carrier",
                "is_seller",
                "postal_code",
                "street",
                "street_number",
                "city",
                "state",
                "tags",
                "contacts"
            ]
        );
    }

    #[tokio::test]
    async fn a_party_change_must_name_its_roles_and_refusals_are_json() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = parties_uri(company_id);
        let created = send(
            &app,
            json_request("POST", &uri, Some(&cookie), party_body()),
        )
        .await;
        let one = format!("{uri}/{}", created.body["id"].as_str().unwrap());
        let put = |body: Value| json_request("PUT", &one, Some(&cookie), body);

        // Replacing without the new roles would drop them.
        let reply = send(&app, put(party_body())).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            reply.body["error"]
                .as_str()
                .unwrap()
                .contains("is_carrier is required")
        );

        // The same for the lists: a client that does not know them would clear them.
        let mut roles_only = party_body();
        roles_only["is_carrier"] = json!(false);
        roles_only["is_seller"] = json!(false);
        let reply = send(&app, put(roles_only.clone())).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            reply.body["error"]
                .as_str()
                .unwrap()
                .contains("contacts is required")
        );
        roles_only["contacts"] = json!([]);
        let reply = send(&app, put(roles_only)).await;
        assert!(
            reply.body["error"]
                .as_str()
                .unwrap()
                .contains("tags is required")
        );

        let mut complete = party_body();
        complete["is_carrier"] = json!(false);
        complete["is_seller"] = json!(false);
        complete["contacts"] = json!([]);
        complete["tags"] = json!([]);

        for (field, value, expected) in [
            ("tags", json!(["a,b"]), "tag"),
            (
                "tags",
                json!((0..11).map(|n| format!("t{n}")).collect::<Vec<_>>()),
                "10 tags",
            ),
            ("contacts", json!([{ "name": "" }]), "contact name"),
            (
                "contacts",
                json!([{ "name": "Ana", "phone": "123" }]),
                "phone",
            ),
            (
                "contacts",
                json!([{ "name": "Ana", "email": "ana" }]),
                "email",
            ),
        ] {
            let mut body = complete.clone();
            body[field] = value;
            let reply = send(&app, put(body)).await;
            assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{field}");
            assert!(reply.body["error"].as_str().unwrap().contains(expected));
        }
        let reply = send(&app, get_request(&format!("{uri}?tag=a,b"), Some(&cookie))).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST);
        // A blank tag is no filter, like a blank search.
        let reply = send(&app, get_request(&format!("{uri}?tag=%20"), Some(&cookie))).await;
        assert_eq!(reply.status, StatusCode::OK);

        let mut bad_address = complete.clone();
        bad_address["address"] = address_body();
        bad_address["address"]["postal_code"] = json!("123");
        let reply = send(&app, put(bad_address)).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(reply.body["error"].as_str().unwrap().contains("CEP"));

        let mut no_role = complete.clone();
        no_role["is_customer"] = json!(false);
        let reply = send(&app, put(no_role)).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);

        // A field the API does not know is an error, not silently dropped (a typo in `address`).
        let mut typo = complete.clone();
        typo["adress"] = address_body();
        assert!(send(&app, put(typo)).await.status.is_client_error());

        // A document another party already has.
        let other = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                json!({ "name": "Bruno", "document": "529.982.247-25", "is_customer": true, "is_supplier": false }),
            ),
        )
        .await;
        let mut taken = complete;
        taken["document"] = json!("529.982.247-25");
        let reply = send(&app, put(taken)).await;
        assert_eq!(reply.status, StatusCode::CONFLICT);
        assert_eq!(other.status, StatusCode::CREATED);

        // Nothing above changed the party.
        let history = send(&app, get_request(&format!("{one}/history"), Some(&cookie))).await;
        assert_eq!(history.body.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn parties_cannot_be_changed_from_another_company() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;
        let created = send(
            &app,
            json_request(
                "POST",
                &parties_uri(company_id),
                Some(&cookie),
                party_body(),
            ),
        )
        .await;
        let id = created.body["id"].as_str().unwrap();
        let mut body = party_body();
        body["is_carrier"] = json!(false);
        body["is_seller"] = json!(false);
        body["contacts"] = json!([]);
        body["tags"] = json!([]);

        let theirs = format!("{}/{id}", parties_uri(stranger_company));
        let put = json_request("PUT", &theirs, Some(&stranger_cookie), body.clone());
        assert_eq!(send(&app, put).await.status, StatusCode::NOT_FOUND);
        let mine = format!("{}/{id}", parties_uri(company_id));
        let put = json_request("PUT", &mine, Some(&stranger_cookie), body.clone());
        assert_eq!(send(&app, put).await.status, StatusCode::NOT_FOUND);
        let put = json_request("PUT", &mine, None, body);
        assert_eq!(send(&app, put).await.status, StatusCode::UNAUTHORIZED);
    }

    fn postal_codes_uri(company_id: Uuid, code: &str) -> String {
        format!("/companies/{company_id}/postal-codes/{code}")
    }

    #[tokio::test]
    async fn finds_the_address_of_a_postal_code_with_or_without_the_hyphen() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let expected = json!({
            "street": "Avenida Paulista",
            "district": "Bela Vista",
            "city": "São Paulo",
            "state": "SP"
        });
        for code in ["01310-100", "01310100"] {
            let reply = send(
                &app,
                get_request(&postal_codes_uri(company_id, code), Some(&cookie)),
            )
            .await;
            assert_eq!(reply.status, StatusCode::OK, "{code}");
            assert_eq!(reply.body, expected, "{code}");
        }
        // A city with one code for all of it has no street.
        let reply = send(
            &app,
            get_request(&postal_codes_uri(company_id, "55555-000"), Some(&cookie)),
        )
        .await;
        assert_eq!(reply.body["street"], Value::Null);
        assert_eq!(reply.body["city"], "Cidade Pequena");
    }

    #[tokio::test]
    async fn a_postal_code_lookup_needs_a_session_and_a_company_of_the_caller() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        let uri = postal_codes_uri(company_id, "01310-100");
        assert_eq!(
            send(&app, get_request(&uri, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(&app, get_request(&uri, Some(&stranger))).await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&uri, Some(&cookie))).await.status,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn a_postal_code_that_is_not_eight_digits_is_refused_without_asking_the_directory() {
        // An unreachable directory would answer 503: a 422 proves it was never asked.
        let app = app_with(
            PartyService::new(InMemoryPartyRepository::default(), known_companies()),
            InMemoryPostalCodeDirectory::unreachable(),
        );
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        for code in ["123", "0131010a", "013101000"] {
            let reply = send(
                &app,
                get_request(&postal_codes_uri(company_id, code), Some(&cookie)),
            )
            .await;
            assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{code}");
            assert!(reply.body["error"].as_str().unwrap().contains("CEP"));
        }
    }

    #[tokio::test]
    async fn an_unknown_postal_code_is_a_404_and_an_unreachable_directory_a_503() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let reply = send(
            &app,
            get_request(&postal_codes_uri(company_id, "99999-999"), Some(&cookie)),
        )
        .await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
        assert!(
            reply.body["error"]
                .as_str()
                .unwrap()
                .contains("fill the address in by hand")
        );

        let down = app_with(
            PartyService::new(InMemoryPartyRepository::default(), known_companies()),
            InMemoryPostalCodeDirectory::unreachable(),
        );
        let (cookie, company_id) = sign_up(&down, "ana@example.com").await;
        let reply = send(
            &down,
            get_request(&postal_codes_uri(company_id, "01310-100"), Some(&cookie)),
        )
        .await;
        assert_eq!(reply.status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn a_postal_code_lookup_switched_off_is_a_409() {
        let app = router::<NoLookupPorts>(
            Services {
                bank_sync: standalone_bank_sync(),
                webhook_subscriptions: standalone_hooks(),
                api_keys: standalone_api_keys(),
                connectors: InMemoryConnectorRepository::default(),
                sale_claims: InMemorySaleClaimRepository::default(),
                parties: PartyService::new(InMemoryPartyRepository::default(), None),
                products: ProductService::new(InMemoryProductRepository::default()),
                cost_centers: CostCenterService::new(InMemoryCostCenterRepository::default()),
                categories: CategoryService::new(InMemoryCategoryRepository::default()),
                bank_accounts: BankAccountService::new(InMemoryBankAccountRepository::default()),
                payment_methods: PaymentMethodService::new(
                    InMemoryPaymentMethodRepository::default(),
                ),
                payment_terms: PaymentTermService::new(InMemoryPaymentTermRepository::default()),
                entries: standalone_entries(),
                attachments: standalone_attachments(),
                charges: standalone_charges(),
                webhooks: standalone_webhooks(),
                payment_pages: standalone_payment_pages(),
                dunning: standalone_dunning(),
                collection: standalone_collection(),
                transfers: standalone_transfers(),
                recurrences: standalone_recurrences(),
                quotes: standalone_quotes(),
                sales: standalone_sales(),
                purchases: standalone_purchases(),
                stock: standalone_stock(),
                counts: standalone_counts(),
                contracts: ContractService::new(
                    InMemoryContractRepository::default(),
                    standalone_sales(),
                ),
                commissions: standalone_commissions(),
                price_lists: standalone_price_lists(),
                sales_reports: SalesReportService::new(InMemorySaleRepository::default()),
                reconciliation: standalone_reconciliation(),
                balances: standalone_balances(),
                period_locks: PeriodLockService::new(locks()),
                exports: standalone_exports(),
                packages: standalone_packages(),
                fiscal_documents: standalone_fiscal_documents(),
                fiscal_settings: FiscalSettingsService::new(
                    InMemoryFiscalSettingsRepository::default(),
                ),
                gateway_settings: test_gateway_settings(),
                dre: DreService::new(
                    InMemoryFinancialEntryRepository::default(),
                    InMemoryCategoryRepository::default(),
                ),
                postal_codes: PostalCodeService::new(None),
                members: standalone_members(),
                documents: standalone_documents(),
                auth: auth_service(),
            },
            ApiConfig::default(),
        );
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let reply = send(
            &app,
            get_request(&postal_codes_uri(company_id, "01310-100"), Some(&cookie)),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CONFLICT);
    }

    fn csv_request(uri: &str, cookie: Option<&str>, body: impl Into<Body>) -> Request<Body> {
        let mut builder = Request::post(uri).header(header::CONTENT_TYPE, "text/csv");
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        builder.body(body.into()).unwrap()
    }

    fn import_uri(company_id: Uuid, query: &str) -> String {
        format!("/companies/{company_id}/parties/import{query}")
    }

    #[tokio::test]
    async fn imports_parties_from_a_csv_file_with_semicolons_roles_tags_and_an_address() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let file = "\u{feff}Name;Document;Email;Customer;Supplier;Postal_Code;Street;City;State;Tags\n\
                    Padaria Central;11.222.333/0001-81;contato@padaria.example;sim;;;;;;VIP|atacado\n\
                    Ana Souza;529.982.247-25;;sim;não;01310-100;Avenida Paulista;São Paulo;sp;\n\
                    \"Silva; Filhos\";;;;s;;;;;\n";
        let reply = send(
            &app,
            csv_request(&import_uri(company_id, ""), Some(&cookie), file),
        )
        .await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
        assert_eq!(reply.body["created"], 3);
        assert_eq!(reply.body["valid"], 3);
        assert_eq!(reply.body["dry_run"], false);
        assert_eq!(reply.body["refused"], json!([]));

        let listed = send(&app, get_request(&parties_uri(company_id), Some(&cookie))).await;
        let items = listed.body["items"].as_array().unwrap();
        let names: Vec<&str> = items.iter().map(|i| i["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["Ana Souza", "Padaria Central", "Silva; Filhos"]);
        assert_eq!(items[0]["address"]["state"], "SP");
        assert_eq!(items[0]["address"]["postal_code"], "01310100");
        assert_eq!(items[1]["tags"], json!(["atacado", "vip"]));
        assert_eq!(items[2]["is_supplier"], true);
        // The registry knows this CNPJ, but the import does not ask it: the check stays open.
        assert_eq!(items[1]["document_check"]["status"], "unverified");
        assert_eq!(items[0]["document_check"]["status"], "not_applicable");

        let history = send(
            &app,
            get_request(
                &format!(
                    "{}/{}/history",
                    parties_uri(company_id),
                    items[0]["id"].as_str().unwrap()
                ),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(history.body.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_dry_run_and_a_file_with_a_bad_row_write_nothing_and_name_every_bad_row() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let good = "name,document,customer\nAna,529.982.247-25,sim\n";
        let dry = send(
            &app,
            csv_request(
                &import_uri(company_id, "?dry_run=true"),
                Some(&cookie),
                good,
            ),
        )
        .await;
        assert_eq!(dry.body["dry_run"], true);
        assert_eq!(
            (dry.body["valid"].clone(), dry.body["created"].clone()),
            (json!(1), json!(0))
        );

        let bad = "name,document,customer,supplier\n\
                   Ana,529.982.247-25,sim,\n\
                   Bruno,111.111.111-11,sim,\n\
                   Ana de novo,52998224725,sim,\n\
                   Carla,,talvez,\n\
                   Dani,,,\n";
        let reply = send(
            &app,
            csv_request(&import_uri(company_id, ""), Some(&cookie), bad),
        )
        .await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(reply.body["created"], 0);
        assert_eq!(
            reply.body["refused"],
            json!([
                { "row": 3, "message": "document is not a valid CPF or CNPJ" },
                { "row": 4, "message": "the same document as row 2" },
                { "row": 5, "message": "customer must be sim or não" },
                { "row": 6, "message": "party must be a customer, a supplier, a carrier or a seller" },
            ])
        );
        let listed = send(&app, get_request(&parties_uri(company_id), Some(&cookie))).await;
        assert!(listed.body["items"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_row_with_an_unquoted_separator_is_refused_for_its_row_and_blank_rows_are_ignored() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let file = "name;customer;\nAna;sim;\nSilva; Filhos;sim;\n;;\nBruno;sim;\n";
        let reply = send(
            &app,
            csv_request(&import_uri(company_id, ""), Some(&cookie), file),
        )
        .await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
        assert_eq!(reply.body["created"], 0);
        let refused = reply.body["refused"].as_array().unwrap();
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0]["row"], 3);
        assert!(
            refused[0]["message"]
                .as_str()
                .unwrap()
                .contains("goes in quotes")
        );

        // Quoted, and with the empty row and the empty column left out, the file goes through and
        // keeps the numbers a spreadsheet shows.
        let fixed = "name;customer;\nAna;sim;\n\"Silva; Filhos\";sim;\n;;\nBruno;sim;\n";
        let reply = send(
            &app,
            csv_request(&import_uri(company_id, ""), Some(&cookie), fixed),
        )
        .await;
        assert_eq!(reply.body["created"], 3);
    }

    #[tokio::test]
    async fn a_document_already_registered_is_skipped_and_never_overwritten() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let mut body = party_body();
        body["document"] = json!("529.982.247-25");
        body["name"] = json!("Ana original");
        send(
            &app,
            json_request("POST", &parties_uri(company_id), Some(&cookie), body),
        )
        .await;

        let file = "name,document,customer\nAna do arquivo,529.982.247-25,sim\nBruno,,sim\n";
        let reply = send(
            &app,
            csv_request(&import_uri(company_id, ""), Some(&cookie), file),
        )
        .await;
        assert_eq!(reply.body["created"], 1);
        assert_eq!(
            reply.body["skipped"],
            json!([{ "row": 2, "message": "a party with this document already exists" }])
        );
        let listed = send(&app, get_request(&parties_uri(company_id), Some(&cookie))).await;
        let names: Vec<&str> = listed.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Ana original", "Bruno"]);
    }

    #[tokio::test]
    async fn a_file_that_cannot_be_read_is_refused_with_what_to_change() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let post = |query: &str, body: Vec<u8>| {
            csv_request(&import_uri(company_id, query), Some(&cookie), body)
        };
        let cases: [(Vec<u8>, &str); 4] = [
            (
                b"name,customer,phone\nAna,sim,1\n".to_vec(),
                "does not know: phone",
            ),
            (
                b"name,document\nAna,\n".to_vec(),
                "at least one of the columns customer",
            ),
            (vec![0xff, 0xfe, b'n'], "not UTF-8"),
            (b"name,customer\n".to_vec(), "no rows"),
        ];
        for (file, expected) in cases {
            let reply = send(&app, post("", file)).await;
            assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{expected}");
            assert!(
                reply.body["error"].as_str().unwrap().contains(expected),
                "{expected}"
            );
        }
        let too_big = send(&app, post("", vec![b'a'; 1024 * 1024 + 1])).await;
        assert_eq!(too_big.status, StatusCode::PAYLOAD_TOO_LARGE);
        assert!(too_big.body["error"].as_str().unwrap().contains("1 MiB"));
        let typo = send(
            &app,
            post("?dryrun=true", b"name,customer\nAna,sim\n".to_vec()),
        )
        .await;
        assert_eq!(typo.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn an_import_needs_a_session_and_a_company_of_the_caller() {
        let app = app();
        let (_, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        let file = "name,customer\nAna,sim\n";
        let uri = import_uri(company_id, "");
        assert_eq!(
            send(&app, csv_request(&uri, None, file)).await.status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(&app, csv_request(&uri, Some(&stranger), file))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }

    /// An app whose identities the test keeps a handle to, to give members the roles that no route
    /// hands out yet.
    fn app_with_identities() -> (Router, InMemoryIdentityRepository) {
        let (app, identities, _) = app_with_mailbox();
        (app, identities)
    }

    /// Like `app_with_identities`, and the mailbox the app sends its e-mails to.
    fn app_with_mailbox() -> (Router, InMemoryIdentityRepository, InMemoryEmailSender) {
        let identities = InMemoryIdentityRepository::default();
        let emails = InMemoryEmailSender::default();
        let auth = AuthService::new(
            identities.clone(),
            Argon2SecretService::new().unwrap(),
            Duration::from_secs(3600),
        );
        let app = app_with_auth(
            PartyService::new(InMemoryPartyRepository::default(), known_companies())
                .with_recheck_interval(Duration::ZERO),
            known_postal_codes(),
            auth,
            members_over(&identities, &emails),
        );
        (app, identities, emails)
    }

    /// The cookie of a new user who is a member of `company_id` with `role`.
    async fn member_with_role(
        app: &Router,
        identities: &InMemoryIdentityRepository,
        company_id: Uuid,
        role: conta_verde_domain::Role,
    ) -> String {
        let email = format!("{}-{}@example.com", role.as_str(), Uuid::new_v4());
        let (cookie, _) = sign_up(app, &email).await;
        let me = send(app, get_request("/auth/me", Some(&cookie))).await;
        let user_id: Uuid = me.body["user"]["id"].as_str().unwrap().parse().unwrap();
        identities
            .add_membership(
                conta_verde_domain::UserId::from_uuid(user_id),
                conta_verde_domain::CompanyId::from_uuid(company_id),
                role,
            )
            .await
            .unwrap();
        cookie
    }

    /// Every route of a company in the router, with the methods it answers, read from the source of
    /// `router` itself so that a route added without a decision is walked by the tests below.
    fn company_routes() -> Vec<(String, Vec<&'static str>)> {
        let source = include_str!("lib.rs");
        let source = &source[..source.find("#[cfg(test)]").unwrap()];
        let mut routes = Vec::new();
        for (at, _) in source.match_indices(".route(") {
            let body = &source[at + ".route(".len()..];
            let mut depth = 1;
            let mut end = body.len();
            for (index, character) in body.char_indices() {
                match character {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = index;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let body = &body[..end];
            let opening = body.find('"').unwrap();
            let closing = opening + 1 + body[opening + 1..].find('"').unwrap();
            let path = &body[opening + 1..closing];
            if !path.starts_with("/companies/{company_id}") {
                continue;
            }
            let chain = &body[closing + 1..];
            let methods: Vec<&'static str> = [
                ("get", "GET"),
                ("post", "POST"),
                ("put", "PUT"),
                ("patch", "PATCH"),
                ("delete", "DELETE"),
            ]
            .into_iter()
            .filter(|(name, _)| {
                chain.match_indices(&format!("{name}(")).any(|(index, _)| {
                    !chain[..index]
                        .chars()
                        .next_back()
                        .is_some_and(|before| before.is_alphanumeric() || before == '_')
                })
            })
            .map(|(_, method)| method)
            .collect();
            assert!(!methods.is_empty(), "no method found for {path}");
            routes.push((path.to_owned(), methods));
        }
        routes
    }

    #[test]
    fn the_route_walker_finds_the_routes_of_the_router() {
        let routes = company_routes();
        assert!(routes.len() > 100, "{} routes", routes.len());
        let entries = routes
            .iter()
            .find(|(path, _)| path == "/companies/{company_id}/entries")
            .unwrap();
        assert_eq!(entries.1, ["GET", "POST"]);
        let party = routes
            .iter()
            .find(|(path, _)| path == "/companies/{company_id}/parties/{party_id}")
            .unwrap();
        assert_eq!(party.1, ["PUT"]);
        // No company route was missed by the reader (a route built another way would not be walked), and
        // every route belongs to an area.
        let source = include_str!("lib.rs");
        let source = &source[..source.find("#[cfg(test)]").unwrap()];
        assert_eq!(
            routes.len(),
            source.matches("\"/companies/{company_id}").count(),
            "a company route is registered in a way the walker does not read"
        );
        for (path, _) in &routes {
            assert!(access::area_of(path).is_some(), "{path} has no area");
        }
    }

    /// Whatever the area, a role that only looks reads and a role that closes months writes the
    /// accounting area and nothing else ([ADR 0067]): every route of the router, every method it
    /// answers, written out here apart from the domain's table.
    #[tokio::test]
    async fn the_accountant_and_the_viewer_read_every_area_and_write_none_but_accounting() {
        use conta_verde_domain::Role;

        let (app, identities) = app_with_identities();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let denied = "your role in this company does not allow this";
        for (role, may_write_accounting) in [(Role::Accountant, true), (Role::Viewer, false)] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            for (pattern, methods) in company_routes() {
                let area = access::area_of(&pattern).unwrap();
                let mut path = pattern.replace("{company_id}", &company_id.to_string());
                while let Some(open) = path.find('{') {
                    let close = path[open..].find('}').unwrap() + open;
                    path.replace_range(open..=close, &Uuid::new_v4().to_string());
                }
                for method in methods {
                    let reply = if method == "GET" {
                        send(&app, get_request(&path, Some(&cookie))).await
                    } else {
                        send(&app, json_request(method, &path, Some(&cookie), json!({}))).await
                    };
                    let reaches = match (area, method == "GET") {
                        (conta_verde_domain::Area::Members, _) => false,
                        (conta_verde_domain::Area::Accounting, false) => may_write_accounting,
                        (conta_verde_domain::Area::Fiscal, false) => false,
                        (_, reading) => reading,
                    };
                    if reaches {
                        assert_ne!(
                            reply.status,
                            StatusCode::FORBIDDEN,
                            "{role:?} {method} {pattern}"
                        );
                    } else {
                        assert_eq!(
                            reply.status,
                            StatusCode::FORBIDDEN,
                            "{role:?} {method} {pattern}"
                        );
                        assert_eq!(reply.body["error"], denied, "{role:?} {method} {pattern}");
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn the_accountant_reads_the_books_of_every_area() {
        use conta_verde_domain::Role;

        let (app, identities) = app_with_identities();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let cookie = member_with_role(&app, &identities, company_id, Role::Accountant).await;
        for path in [
            "/parties",
            "/products",
            "/categories",
            "/cost-centers",
            "/bank-accounts",
            "/payment-methods",
            "/payment-terms",
            "/entries",
            "/transfers",
            "/recurrences",
            "/statement-lines",
            "/quotes",
            "/sales",
            "/contracts",
            "/commission-rules",
            "/price-lists",
            "/purchase-orders",
            "/inventory-counts",
            "/stock/movements",
            "/stock/positions",
            "/stock/alerts",
        ] {
            let reply = send(
                &app,
                get_request(&format!("/companies/{company_id}{path}"), Some(&cookie)),
            )
            .await;
            assert_eq!(
                reply.status,
                StatusCode::OK,
                "reads {path}: {:?}",
                reply.body
            );
        }
        let me = send(&app, get_request("/auth/me", Some(&cookie))).await;
        let theirs = me.body["companies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|company| company["id"] == company_id.to_string())
            .unwrap();
        let permissions = &theirs["permissions"];
        assert_eq!(permissions["accounting"], "write");
        assert_eq!(permissions["finance"], "read");
        assert_eq!(permissions["purchases"], "read");
        assert!(permissions["members"].is_null());
    }

    fn period_lock_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/accounting/period-lock")
    }

    /// The exit criterion of Phase 6 for the lock: the accountant closes a month from their own login, and from then
    /// on the books of that month refuse changes with a clear `409`, on every route that writes a dated record,
    /// until the month is reopened; the lock is audited and a reader may see it but not change it.
    #[tokio::test]
    async fn an_accountant_closes_a_month_and_the_books_of_it_refuse_changes() {
        use conta_verde_domain::Role;

        let (app, identities) = entry_app_with_identities();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let accountant = member_with_role(&app, &identities, company_id, Role::Accountant).await;
        let viewer = member_with_role(&app, &identities, company_id, Role::Viewer).await;
        let targets = entry_targets(&app, &owner, company_id).await;
        let product = post_to(&app, &products_uri(company_id), &owner, product_body()).await;
        assert_eq!(product.status, StatusCode::CREATED, "{}", product.body);
        let product = product.body["id"].as_str().unwrap().to_owned();
        let account = registered_account(&app, &owner, company_id, "Caixa").await;
        let other_account = registered_account(&app, &owner, company_id, "Banco").await;
        let lock = period_lock_uri(company_id);
        let entry_in = |competence: &str| {
            let mut body = entry_body(&targets, "Fatura", &[("2026-11-30", "100")]);
            body["competence_date"] = json!(competence);
            body
        };
        let closed = "the period through 2026-10-31 is closed: reopen it to change what is dated on or before it";

        // Before anything is closed everything is written, and an October entry and payment exist.
        let october = post_to(
            &app,
            &entries_uri(company_id),
            &owner,
            entry_in("2026-10-15"),
        )
        .await;
        assert_eq!(october.status, StatusCode::CREATED, "{}", october.body);
        let october_id = october.body["id"].as_str().unwrap().to_owned();
        let settlements = format!("{}/{}/settlements", entries_uri(company_id), october_id);
        let payment = post_to(
            &app,
            &settlements,
            &owner,
            json!({ "installment": 1, "settled_on": "2026-10-20", "applied": "40", "bank_account_id": account }),
        )
        .await;
        assert_eq!(payment.status, StatusCode::CREATED, "{}", payment.body);
        let payment_id = payment.body["id"].as_str().unwrap().to_owned();
        let none = send(&app, get_request(&lock, Some(&viewer))).await;
        assert_eq!(none.body["locked_through"], Value::Null);

        // A reader sees the lock and cannot change it; only a whole month can be closed.
        let denied = send(
            &app,
            json_request(
                "PUT",
                &lock,
                Some(&viewer),
                json!({ "locked_through": "2026-10-31" }),
            ),
        )
        .await;
        assert_eq!(denied.status, StatusCode::FORBIDDEN);
        let half = send(
            &app,
            json_request(
                "PUT",
                &lock,
                Some(&accountant),
                json!({ "locked_through": "2026-10-30" }),
            ),
        )
        .await;
        assert_eq!(
            half.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            half.body
        );
        let not_a_date = send(
            &app,
            json_request(
                "PUT",
                &lock,
                Some(&accountant),
                json!({ "locked_through": "october" }),
            ),
        )
        .await;
        assert_eq!(not_a_date.status, StatusCode::UNPROCESSABLE_ENTITY);

        // The accountant closes October.
        let closing = send(
            &app,
            json_request(
                "PUT",
                &lock,
                Some(&accountant),
                json!({ "locked_through": "2026-10-31" }),
            ),
        )
        .await;
        assert_eq!(closing.status, StatusCode::OK, "{}", closing.body);
        assert_eq!(closing.body["locked_through"], "2026-10-31");
        assert_eq!(
            send(&app, get_request(&lock, Some(&viewer))).await.body["locked_through"],
            "2026-10-31"
        );

        // Every write of a dated record in October is a 409 that says why; the first day after is open.
        let refused = |reply: Reply, what: &str| {
            assert_eq!(reply.status, StatusCode::CONFLICT, "{what}: {}", reply.body);
            assert_eq!(reply.body["error"], closed, "{what}");
        };
        refused(
            post_to(
                &app,
                &entries_uri(company_id),
                &owner,
                entry_in("2026-10-31"),
            )
            .await,
            "register an entry",
        );
        refused(
            send(
                &app,
                json_request(
                    "PUT",
                    &format!("{}/{}", entries_uri(company_id), october_id),
                    Some(&owner),
                    {
                        let mut body = entry_in("2026-10-15");
                        body["description"] = json!("Fatura corrigida");
                        body
                    },
                ),
            )
            .await,
            "change an entry",
        );
        refused(
            post_to(
                &app,
                &settlements,
                &owner,
                json!({ "installment": 1, "settled_on": "2026-10-31", "applied": "10", "bank_account_id": account }),
            )
            .await,
            "settle on a closed day",
        );
        refused(
            send(
                &app,
                json_request(
                    "DELETE",
                    &format!("{settlements}/{payment_id}"),
                    Some(&owner),
                    Value::Null,
                ),
            )
            .await,
            "reverse a payment of a closed day",
        );
        refused(
            post_to(
                &app,
                &transfers_uri(company_id),
                &owner,
                json!({ "from_account_id": account, "to_account_id": other_account, "amount": "5", "occurred_on": "2026-10-20" }),
            )
            .await,
            "transfer",
        );
        refused(
            send(
                &app,
                json_request(
                    "PUT",
                    &format!("/companies/{company_id}/bank-accounts/{account}/opening-balance"),
                    Some(&owner),
                    json!({ "balance": "10", "on": "2026-10-01" }),
                ),
            )
            .await,
            "opening balance",
        );
        refused(
            post_to(
                &app,
                &format!("{}/movements", stock_uri(company_id)),
                &owner,
                json!({ "product_id": product, "kind": "in", "quantity": "2", "unit_cost": "5", "occurred_on": "2026-10-10" }),
            )
            .await,
            "stock movement",
        );
        let imported = send(
            &app,
            csv_request(
                &format!("/companies/{company_id}/entries/import"),
                Some(&owner),
                "reference,kind,description,amount,competence_date,due_date,category\n\
                 r-1,receivable,Venda,10,2026-10-20,2026-11-20,Vendas\n",
            ),
        )
        .await;
        assert!(
            imported.body.to_string().contains("is closed"),
            "{}",
            imported.body
        );

        // After the lock, and a payment in November of the October invoice, which changes October nothing.
        for (what, reply) in [
            (
                "an entry of the first day after",
                post_to(&app, &entries_uri(company_id), &owner, entry_in("2026-11-01")).await,
            ),
            (
                "a payment of an October invoice made in November",
                post_to(
                    &app,
                    &settlements,
                    &owner,
                    json!({ "installment": 1, "settled_on": "2026-11-02", "applied": "10", "bank_account_id": account }),
                )
                .await,
            ),
            (
                "a stock movement of the first day after",
                post_to(
                    &app,
                    &format!("{}/movements", stock_uri(company_id)),
                    &owner,
                    json!({ "product_id": product, "kind": "in", "quantity": "2", "unit_cost": "5", "occurred_on": "2026-11-01" }),
                )
                .await,
            ),
        ] {
            assert_eq!(reply.status, StatusCode::CREATED, "{what}: {}", reply.body);
        }

        // A request that does not say the day is refused, not read as reopening everything.
        let silent = send(
            &app,
            json_request(
                "PUT",
                &lock,
                Some(&accountant),
                json!({ "reason": "forgot the day" }),
            ),
        )
        .await;
        assert!(silent.status.is_client_error(), "{}", silent.body);
        assert_eq!(
            send(&app, get_request(&lock, Some(&viewer))).await.body["locked_through"],
            "2026-10-31"
        );

        // Reopening says why; the trail names who closed and who reopened.
        let no_reason = send(
            &app,
            json_request(
                "PUT",
                &lock,
                Some(&accountant),
                json!({ "locked_through": null }),
            ),
        )
        .await;
        assert_eq!(
            no_reason.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            no_reason.body
        );
        let reopened = send(
            &app,
            json_request(
                "PUT",
                &lock,
                Some(&accountant),
                json!({ "locked_through": null, "reason": "a supplier invoice came late" }),
            ),
        )
        .await;
        assert_eq!(reopened.body["locked_through"], Value::Null);
        let again = post_to(
            &app,
            &entries_uri(company_id),
            &owner,
            entry_in("2026-10-31"),
        )
        .await;
        assert_eq!(again.status, StatusCode::CREATED, "{}", again.body);
        let history = send(&app, get_request(&format!("{lock}/history"), Some(&viewer))).await;
        let history = history.body.as_array().unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0]["action"], "created");
        assert_eq!(history[0]["changes"][0]["to"], "2026-10-31");
        assert_eq!(history[1]["action"], "updated");
        assert_eq!(
            history[1]["changes"][1]["to"],
            "a supplier invoice came late"
        );
    }

    #[tokio::test]
    async fn what_a_member_may_do_follows_their_role_on_every_route_family() {
        use conta_verde_domain::Role;

        let (app, identities) = app_with_identities();
        let (owner_cookie, company_id) = sign_up(&app, "dona@example.com").await;
        let base = format!("/companies/{company_id}");
        let party = send(
            &app,
            json_request(
                "POST",
                &parties_uri(company_id),
                Some(&owner_cookie),
                party_body(),
            ),
        )
        .await;
        let party_id = party.body["id"].as_str().unwrap().to_owned();
        let product = send(
            &app,
            json_request(
                "POST",
                &products_uri(company_id),
                Some(&owner_cookie),
                product_body(),
            ),
        )
        .await;
        let product_id = product.body["id"].as_str().unwrap().to_owned();

        // Which of the three kinds of write each role has. Written out here, apart from the domain's
        // own table, so that a change to who may do what is a change to this test too.
        //                      parties products finance
        let writes = [
            (Role::Owner, true, true, true),
            (Role::Admin, true, true, true),
            (Role::Financial, true, false, true),
            (Role::Sales, true, false, false),
            (Role::Stock, false, true, false),
            (Role::Accountant, false, false, false),
            (Role::Viewer, false, false, false),
        ];
        let denied = "your role in this company does not allow this";
        let mut created: Vec<String> = vec!["Padaria Central".into()];
        for (role, parties, products, finance) in writes {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            let name = format!("Cadastro de {}", role.as_str());

            // Looking is for everyone, whatever the area.
            for path in [
                "/parties",
                &format!("/parties/{party_id}/history"),
                "/products",
                "/categories",
                "/cost-centers",
                "/bank-accounts",
                "/payment-methods",
                "/payment-terms",
                "/postal-codes/01310-100",
            ] {
                let reply = send(&app, get_request(&format!("{base}{path}"), Some(&cookie))).await;
                assert_eq!(reply.status, StatusCode::OK, "{role:?} reads {path}");
            }

            let party_write = |method: &str, path: &str, body: Value| {
                json_request(method, &format!("{base}{path}"), Some(&cookie), body)
            };
            let mut body = party_body();
            body["document"] = Value::Null;
            body["name"] = json!(name);
            let ok = StatusCode::OK;
            let created_status = StatusCode::CREATED;
            let writes_of = [
                (
                    parties,
                    "creates a party",
                    party_write("POST", "/parties", body),
                    vec![created_status],
                ),
                (
                    parties,
                    "imports parties",
                    csv_request(
                        &format!("{base}/parties/import?dry_run=true"),
                        Some(&cookie),
                        "name,customer\nAna,sim\n",
                    ),
                    vec![ok],
                ),
                (
                    parties,
                    "checks a CNPJ again",
                    party_write(
                        "POST",
                        &format!("/parties/{party_id}/document-check"),
                        Value::Null,
                    ),
                    vec![ok],
                ),
                (
                    products,
                    "creates a product",
                    party_write("POST", "/products", {
                        let mut product = product_body();
                        product["sku"] = json!(format!("{}-1", role.as_str()));
                        product
                    }),
                    vec![created_status],
                ),
                (
                    products,
                    "changes a product",
                    party_write("PUT", &format!("/products/{product_id}"), {
                        let mut product = product_body();
                        product["active"] = json!(true);
                        product
                    }),
                    vec![ok],
                ),
                (
                    products,
                    "imports products",
                    csv_request(
                        &format!("{base}/products/import?dry_run=true"),
                        Some(&cookie),
                        "name,unit\nPão,kg\n",
                    ),
                    vec![ok],
                ),
                (
                    finance,
                    "creates a cost center",
                    party_write(
                        "POST",
                        "/cost-centers",
                        json!({ "name": format!("Centro {}", role.as_str()) }),
                    ),
                    vec![created_status],
                ),
                (
                    finance,
                    "applies the default categories",
                    party_write("POST", "/categories/default-template", Value::Null),
                    // The first role that may do it does; for the others they already exist.
                    vec![created_status, StatusCode::CONFLICT],
                ),
            ];
            for (allowed, what, request, statuses) in writes_of {
                let reply = send(&app, request).await;
                if allowed {
                    assert!(
                        statuses.contains(&reply.status),
                        "{role:?} {what}: {} {}",
                        reply.status,
                        reply.body
                    );
                } else {
                    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{role:?} {what}");
                    assert_eq!(reply.body["error"], denied, "{role:?} {what}");
                }
            }
            if parties {
                created.push(name.clone());
            }

            // A change to the party is a write too, and so is checking its CNPJ again.
            let update = send(
                &app,
                json_request(
                    "PUT",
                    &format!("{base}/parties/{party_id}"),
                    Some(&cookie),
                    json!({
                        "name": "Padaria Central",
                        "document": "11.222.333/0001-81",
                        "is_customer": true,
                        "is_supplier": false,
                        "is_carrier": false,
                        "is_seller": false,
                        "contacts": [],
                        "tags": []
                    }),
                ),
            )
            .await;
            assert_eq!(
                update.status == StatusCode::FORBIDDEN,
                !parties,
                "{role:?} PUT"
            );
        }

        // What was refused left nothing behind: only the roles that may write parties added one.
        let listed = send(
            &app,
            get_request(&parties_uri(company_id), Some(&owner_cookie)),
        )
        .await;
        let mut names: Vec<&str> = listed.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect();
        names.sort_unstable();
        let mut expected: Vec<&str> = created.iter().map(String::as_str).collect();
        expected.sort_unstable();
        assert_eq!(names, expected);
        assert_eq!(created.len(), 1 + 4);

        // The same for products (the owner's, and one per role that may write them) and for the
        // cost centers (one per role that may write the finance registers).
        let skus_of = |reply: &Reply| -> Vec<String> {
            let mut skus: Vec<String> = reply.body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["sku"].as_str().unwrap().to_owned())
                .collect();
            skus.sort_unstable();
            skus
        };
        let products = send(
            &app,
            get_request(&products_uri(company_id), Some(&owner_cookie)),
        )
        .await;
        assert_eq!(
            skus_of(&products),
            ["ADMIN-1", "OWNER-1", "PAO-001", "STOCK-1"],
            "{}",
            products.body
        );
        let centers = send(
            &app,
            get_request(&format!("{base}/cost-centers"), Some(&owner_cookie)),
        )
        .await;
        let mut centers: Vec<&str> = centers.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect();
        centers.sort_unstable();
        assert_eq!(
            centers,
            ["Centro admin", "Centro financial", "Centro owner"]
        );
    }

    #[tokio::test]
    async fn the_profile_says_what_the_caller_is_and_how_far_it_reaches() {
        use conta_verde_domain::Role;

        let (app, identities) = app_with_identities();
        let (owner_cookie, company_id) = sign_up(&app, "dona@example.com").await;
        let me = send(&app, get_request("/auth/me", Some(&owner_cookie))).await;
        assert_eq!(me.body["companies"][0]["role"], "owner");
        assert_eq!(
            me.body["companies"][0]["permissions"],
            json!({ "parties": "write", "products": "write", "finance": "write", "sales": "write", "purchases": "write", "accounting": "write", "fiscal": "write", "members": "write" })
        );

        let viewer = member_with_role(&app, &identities, company_id, Role::Viewer).await;
        let me = send(&app, get_request("/auth/me", Some(&viewer))).await;
        // Their own company first (by name, both are "Padaria"), then the one they were added to:
        // find the one they were added to by its id.
        let theirs = me.body["companies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|company| company["id"] == company_id.to_string())
            .unwrap();
        assert_eq!(theirs["role"], "viewer");
        // Members is out of reach, so the area is not there at all.
        assert_eq!(
            theirs["permissions"],
            json!({ "parties": "read", "products": "read", "finance": "read", "sales": "read", "purchases": "read", "accounting": "read", "fiscal": "read" })
        );
    }

    #[tokio::test]
    async fn a_non_member_is_still_told_the_company_does_not_exist_not_that_they_may_not() {
        let (app, _) = app_with_identities();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let (stranger, _) = sign_up(&app, "intrusa@example.com").await;
        let reply = send(&app, get_request(&parties_uri(company_id), Some(&stranger))).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
    }

    fn invitations_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/invitations")
    }

    fn token_of(link: &Value) -> String {
        link.as_str()
            .unwrap()
            .rsplit('/')
            .next()
            .unwrap()
            .to_owned()
    }

    async fn invite_as(
        app: &Router,
        cookie: &str,
        company_id: Uuid,
        email: &str,
        role: &str,
    ) -> Reply {
        send(
            app,
            json_request(
                "POST",
                &invitations_uri(company_id),
                Some(cookie),
                json!({ "email": email, "role": role }),
            ),
        )
        .await
    }

    #[tokio::test]
    async fn a_sign_up_for_an_address_nobody_confirmed_sends_its_owner_the_way_to_take_the_account_over()
     {
        let (app, _, mailbox) = app_with_mailbox();
        sign_up(&app, "ana@example.com").await;
        mailbox.clear();
        let again = |password: &str| {
            send(
                &app,
                json_request(
                    "POST",
                    "/auth/register",
                    None,
                    json!({ "name": "Outra", "email": "ana@example.com", "password": password, "company_name": "Outra empresa" }),
                ),
            )
        };

        // The address is held by an account that never confirmed it: refused, and the address gets a recovery link.
        let refused = again("another pass 1").await;
        assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.body);
        assert!(
            refused.body["error"]
                .as_str()
                .unwrap()
                .contains("never confirmed")
        );
        let sent = mailbox.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to.as_str(), "ana@example.com");
        assert_eq!(sent[0].subject, "Redefinição de senha no conta-verde");
        let token = sent[0]
            .body
            .lines()
            .find(|line| line.contains("/reset-password/"))
            .unwrap()
            .rsplit('/')
            .next()
            .unwrap()
            .to_owned();
        // Asking again at once does not mail it again.
        assert_eq!(again("another pass 1").await.status, StatusCode::CONFLICT);
        assert_eq!(mailbox.sent().len(), 1);

        // Whoever has the mailbox takes the account over by choosing a password; the address is confirmed.
        let taken = send(
            &app,
            json_request(
                "POST",
                "/auth/password-reset/confirm",
                None,
                json!({ "token": token, "password": "mine now 123" }),
            ),
        )
        .await;
        assert_eq!(taken.status, StatusCode::NO_CONTENT);
        // Now it is held by a confirmed account: a plain refusal, and nothing is sent.
        mailbox.clear();
        let plain = again("another pass 1").await;
        assert_eq!(plain.status, StatusCode::CONFLICT);
        assert!(
            !plain.body["error"]
                .as_str()
                .unwrap()
                .contains("never confirmed")
        );
        assert!(mailbox.sent().is_empty());
    }

    #[tokio::test]
    async fn an_address_is_confirmed_through_an_e_mailed_link_that_works_once() {
        let (app, _, mailbox) = app_with_mailbox();
        let (cookie, _) = sign_up(&app, "ana@example.com").await;

        // The sign-up sent the link and says the address is not confirmed yet.
        let sent = mailbox.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to.as_str(), "ana@example.com");
        assert_eq!(sent[0].subject, "Confirme seu e-mail no conta-verde");
        let token = sent[0]
            .body
            .lines()
            .find(|line| line.contains("/confirm-email/"))
            .unwrap()
            .rsplit('/')
            .next()
            .unwrap()
            .to_owned();
        let me = |cookie: &str| send(&app, get_request("/auth/me", Some(cookie)));
        assert_eq!(me(&cookie).await.body["email_verified"], false);

        // Asking again needs a session, and one a minute at most is made.
        let resend = |cookie: Option<&str>| {
            send(
                &app,
                json_request("POST", "/auth/email-confirmation", cookie, Value::Null),
            )
        };
        assert_eq!(resend(None).await.status, StatusCode::UNAUTHORIZED);
        assert_eq!(resend(Some(&cookie)).await.status, StatusCode::ACCEPTED);
        assert_eq!(mailbox.sent().len(), 1, "too soon after the last one");

        let confirm = |token: &str| {
            send(
                &app,
                json_request(
                    "POST",
                    "/auth/email-confirmation/confirm",
                    None,
                    json!({ "token": token }),
                ),
            )
        };
        assert_eq!(confirm("wrong").await.status, StatusCode::GONE);
        assert_eq!(confirm(&token).await.status, StatusCode::NO_CONTENT);
        assert_eq!(me(&cookie).await.body["email_verified"], true);
        assert_eq!(confirm(&token).await.status, StatusCode::GONE);
        // Confirmed: nothing more is sent.
        assert_eq!(resend(Some(&cookie)).await.status, StatusCode::ACCEPTED);
        assert_eq!(mailbox.sent().len(), 1);
    }

    #[tokio::test]
    async fn a_lost_password_is_replaced_through_an_e_mailed_link_that_works_once() {
        let (app, _, mailbox) = app_with_mailbox();
        let (cookie, _) = sign_up(&app, "ana@example.com").await;
        // The sign-up sent its own e-mail (the address to confirm): start from an empty mailbox.
        mailbox.clear();
        let ask = |email: &str| {
            json_request(
                "POST",
                "/auth/password-reset",
                None,
                json!({ "email": email }),
            )
        };

        // Any address gets the same answer, and only an account gets an e-mail.
        for email in ["nobody@example.com", "not an address"] {
            assert_eq!(send(&app, ask(email)).await.status, StatusCode::ACCEPTED);
        }
        assert!(mailbox.sent().is_empty());
        let asked = send(&app, ask("ana@example.com")).await;
        assert_eq!(asked.status, StatusCode::ACCEPTED);
        assert!(asked.body.is_null(), "the link is only in the e-mail");
        let sent = mailbox.sent();
        assert_eq!(sent.len(), 1);
        let link = sent[0]
            .body
            .lines()
            .find(|l| l.contains("/reset-password/"))
            .unwrap();
        let token = link.rsplit('/').next().unwrap().to_owned();

        // Another request right after makes no second e-mail.
        send(&app, ask("ana@example.com")).await;
        assert_eq!(mailbox.sent().len(), 1);

        let confirm = |token: &str, password: &str| {
            json_request(
                "POST",
                "/auth/password-reset/confirm",
                None,
                json!({ "token": token, "password": password }),
            )
        };
        assert_eq!(
            send(&app, confirm(&token, "short")).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            send(&app, confirm("wrong", "battery staple")).await.status,
            StatusCode::GONE
        );
        assert_eq!(
            send(&app, confirm(&token, "battery staple")).await.status,
            StatusCode::NO_CONTENT
        );
        // The owner of the address is told that the password changed, and where to go if it was not them.
        let notice = mailbox.sent().pop().unwrap();
        assert_eq!(notice.to.as_str(), "ana@example.com");
        assert_eq!(notice.subject, "Sua senha no conta-verde foi alterada");
        assert!(notice.body.contains("/forgot-password"), "{}", notice.body);
        // The link is spent, the old session is over and the new password signs in.
        assert_eq!(
            send(&app, confirm(&token, "another one 123")).await.status,
            StatusCode::GONE
        );
        assert_eq!(
            send(&app, get_request("/auth/me", Some(&cookie)))
                .await
                .status,
            StatusCode::UNAUTHORIZED
        );
        let login = |password: &str| {
            json_request(
                "POST",
                "/auth/login",
                None,
                json!({ "email": "ana@example.com", "password": password }),
            )
        };
        assert_eq!(
            send(&app, login("correct horse")).await.status,
            StatusCode::UNAUTHORIZED
        );
        let signed_in = send(&app, login("battery staple")).await;
        assert_eq!(signed_in.status, StatusCode::OK);
        // A link that reached the mailbox confirmed the address too.
        assert_eq!(signed_in.body["email_verified"], true);
    }

    #[tokio::test]
    async fn an_owner_invites_a_person_who_accepts_with_their_own_account_and_gets_the_role() {
        let (app, _, mailbox) = app_with_mailbox();
        let (ana, company_id) = sign_up(&app, "ana@example.com").await;
        let (bia, bia_company) = sign_up(&app, "bia@example.com").await;
        let (cris, _) = sign_up(&app, "cris@example.com").await;

        mailbox.clear();
        let invited = invite_as(&app, &ana, company_id, "Bia@Example.com", "sales").await;
        assert_eq!(invited.status, StatusCode::CREATED, "{}", invited.body);
        assert_eq!(invited.body["role"], "sales");
        assert_eq!(invited.body["email"], "bia@example.com");
        assert_eq!(invited.body["emailed"], true);
        assert_eq!(invited.body["expired"], false);
        assert!(
            invited.body["link"]
                .as_str()
                .unwrap()
                .starts_with("https://erp.example/invitations/")
        );
        let sent = mailbox.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to.as_str(), "bia@example.com");
        assert!(
            sent[0]
                .body
                .contains(invited.body["link"].as_str().unwrap())
        );

        // The invitation is listed, without the link; the company has one member so far.
        let listed = send(&app, get_request(&invitations_uri(company_id), Some(&ana))).await;
        assert_eq!(listed.body.as_array().unwrap().len(), 1);
        assert!(listed.body[0].get("link").is_none());
        let members = send(
            &app,
            get_request(&format!("/companies/{company_id}/members"), Some(&ana)),
        )
        .await;
        assert_eq!(
            members.body,
            json!([{
                "user_id": members.body[0]["user_id"], "name": "Ana", "email": "ana@example.com", "role": "owner"
            }])
        );

        // The link says what it leads to, without a session.
        let token = token_of(&invited.body["link"]);
        let preview = send(&app, get_request(&format!("/invitations/{token}"), None)).await;
        assert_eq!(preview.status, StatusCode::OK);
        assert_eq!(preview.body["company_name"], "Padaria");
        assert_eq!(preview.body["email"], "bia@example.com");
        assert_eq!(preview.body["role"], "sales");

        // Accepting needs a session, and the session of the address it was sent to.
        let accept = |cookie: Option<&str>| {
            json_request(
                "POST",
                &format!("/invitations/{token}/accept"),
                cookie,
                Value::Null,
            )
        };
        assert_eq!(
            send(&app, accept(None)).await.status,
            StatusCode::UNAUTHORIZED
        );
        let wrong = send(&app, accept(Some(&cris))).await;
        assert_eq!(wrong.status, StatusCode::FORBIDDEN);
        assert!(
            wrong.body["error"]
                .as_str()
                .unwrap()
                .contains("another e-mail address")
        );
        let accepted = send(&app, accept(Some(&bia))).await;
        assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.body);
        // Whoever sent the invitation is told it was accepted.
        let notice = mailbox.sent().pop().unwrap();
        assert_eq!(notice.to.as_str(), "ana@example.com");
        assert!(
            notice.subject.contains("aceitou seu convite para Padaria"),
            "{}",
            notice.subject
        );
        assert!(notice.body.contains("bia@example.com"), "{}", notice.body);
        assert_eq!(accepted.body["company_id"], company_id.to_string());

        // Bia now belongs to the company, in the role, and what the role allows applies at once.
        let me = send(&app, get_request("/auth/me", Some(&bia))).await;
        let companies = me.body["companies"].as_array().unwrap();
        assert_eq!(companies.len(), 2);
        let joined = companies
            .iter()
            .find(|c| c["id"] == company_id.to_string())
            .unwrap();
        assert_eq!(joined["role"], "sales");
        let own = companies
            .iter()
            .find(|c| c["id"] == bia_company.to_string())
            .unwrap();
        assert_eq!(own["role"], "owner");
        let base = format!("/companies/{company_id}");
        let new_party = send(
            &app,
            json_request("POST", &format!("{base}/parties"), Some(&bia), party_body()),
        )
        .await;
        assert_eq!(new_party.status, StatusCode::CREATED);
        let new_product = send(
            &app,
            json_request(
                "POST",
                &format!("{base}/products"),
                Some(&bia),
                product_body(),
            ),
        )
        .await;
        assert_eq!(new_product.status, StatusCode::FORBIDDEN);
        let members = send(&app, get_request(&format!("{base}/members"), Some(&ana))).await;
        assert_eq!(members.body.as_array().unwrap().len(), 2);
        assert!(
            send(&app, get_request(&invitations_uri(company_id), Some(&ana)))
                .await
                .body
                .as_array()
                .unwrap()
                .is_empty()
        );

        // The link works once.
        assert_eq!(
            send(&app, accept(Some(&bia))).await.status,
            StatusCode::GONE
        );
        assert_eq!(
            send(&app, get_request(&format!("/invitations/{token}"), None))
                .await
                .status,
            StatusCode::GONE
        );
        assert_eq!(
            send(&app, get_request("/invitations/not-a-token", None))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn an_admin_changes_roles_and_removes_members_but_the_owner_stays_as_they_are() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let members_uri = format!("/companies/{company_id}/members");
        let sales = member_with_role(&app, &identities, company_id, Role::Sales).await;
        let admin = member_with_role(&app, &identities, company_id, Role::Admin).await;
        let (other, other_company) = sign_up(&app, "outra@example.com").await;
        let listed = send(&app, get_request(&members_uri, Some(&owner))).await;
        let id_of = |role: &str| {
            listed
                .body
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["role"] == role)
                .unwrap()["user_id"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let (owner_id, sales_id, admin_id) = (id_of("owner"), id_of("sales"), id_of("admin"));
        let change = |cookie: &str, id: &str, role: &str| {
            send(
                &app,
                json_request(
                    "PATCH",
                    &format!("{members_uri}/{id}"),
                    Some(cookie),
                    json!({ "role": role }),
                ),
            )
        };
        let remove = |cookie: &str, id: &str| {
            send(
                &app,
                json_request(
                    "DELETE",
                    &format!("{members_uri}/{id}"),
                    Some(cookie),
                    Value::Null,
                ),
            )
        };

        // Not a role that reaches the members: refused as for any other write.
        assert_eq!(
            change(&sales, &admin_id, "viewer").await.status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            remove(&sales, &admin_id).await.status,
            StatusCode::FORBIDDEN
        );

        // A role changes, and the person sees it at once.
        assert_eq!(
            change(&admin, &sales_id, "financial").await.status,
            StatusCode::NO_CONTENT
        );
        let me = send(&app, get_request("/auth/me", Some(&sales))).await;
        let role_here = me.body["companies"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == company_id.to_string())
            .unwrap()["role"]
            .clone();
        assert_eq!(role_here, "financial");

        // What cannot be said, and whom there is none to change.
        assert_eq!(
            change(&admin, &sales_id, "boss").await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let owning = change(&admin, &sales_id, "owner").await;
        assert_eq!(owning.status, StatusCode::CONFLICT);
        assert!(owning.body["error"].as_str().unwrap().contains("one owner"));
        let stranger = Uuid::new_v4().to_string();
        assert_eq!(
            change(&admin, &stranger, "viewer").await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            change(&admin, "not-an-id", "viewer").await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            remove(&admin, &stranger).await.status,
            StatusCode::NOT_FOUND
        );
        // A member of another company is no member of this one.
        let theirs = send(
            &app,
            get_request(&format!("/companies/{other_company}/members"), Some(&other)),
        )
        .await;
        let their_id = theirs.body[0]["user_id"].as_str().unwrap().to_owned();
        assert_eq!(
            change(&admin, &their_id, "viewer").await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            remove(&admin, &their_id).await.status,
            StatusCode::NOT_FOUND
        );

        // Nobody, an admin included, changes or removes the owner.
        assert_eq!(
            change(&admin, &owner_id, "viewer").await.status,
            StatusCode::CONFLICT
        );
        assert_eq!(remove(&admin, &owner_id).await.status, StatusCode::CONFLICT);
        assert_eq!(remove(&owner, &owner_id).await.status, StatusCode::CONFLICT);

        // A removed member is out of the company at their next request, and still has their account.
        assert_eq!(
            remove(&admin, &sales_id).await.status,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            send(&app, get_request(&parties_uri(company_id), Some(&sales)))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            remove(&admin, &sales_id).await.status,
            StatusCode::NOT_FOUND
        );
        let me = send(&app, get_request("/auth/me", Some(&sales))).await;
        assert_eq!(me.status, StatusCode::OK);
        assert!(
            me.body["companies"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| c["id"] != company_id.to_string())
        );
        let left = send(&app, get_request(&members_uri, Some(&owner))).await;
        assert_eq!(left.body.as_array().unwrap().len(), 2);

        // And can be invited again.
        let again = invite_as(
            &app,
            &owner,
            company_id,
            me.body["user"]["email"].as_str().unwrap(),
            "viewer",
        )
        .await;
        assert_eq!(again.status, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn only_the_roles_that_reach_members_may_see_or_invite_and_what_is_refused_is_said() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let base = format!("/companies/{company_id}");

        // A viewer and a sales person see nothing of who belongs, and invite nobody.
        for role in [
            Role::Viewer,
            Role::Sales,
            Role::Stock,
            Role::Accountant,
            Role::Financial,
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&format!("{base}/members"), Some(&cookie)))
                    .await
                    .status,
                StatusCode::FORBIDDEN,
                "{role:?}"
            );
            assert_eq!(
                send(
                    &app,
                    get_request(&invitations_uri(company_id), Some(&cookie))
                )
                .await
                .status,
                StatusCode::FORBIDDEN,
                "{role:?}"
            );
            assert_eq!(
                invite_as(&app, &cookie, company_id, "x@example.com", "viewer")
                    .await
                    .status,
                StatusCode::FORBIDDEN,
                "{role:?}"
            );
        }
        // An admin does both.
        let admin = member_with_role(&app, &identities, company_id, Role::Admin).await;
        assert_eq!(
            send(&app, get_request(&format!("{base}/members"), Some(&admin)))
                .await
                .status,
            StatusCode::OK
        );
        assert_eq!(
            invite_as(&app, &admin, company_id, "x@example.com", "viewer")
                .await
                .status,
            StatusCode::CREATED
        );

        // Nobody is invited as an owner, in a role that does not exist, with a bad address, twice, or
        // when already a member.
        let refused = |reply: Reply, status: StatusCode, said: &str| {
            assert_eq!(reply.status, status, "{said}: {}", reply.body);
            assert!(
                reply.body["error"].as_str().unwrap().contains(said),
                "{said}: {}",
                reply.body
            );
        };
        refused(
            invite_as(&app, &owner, company_id, "y@example.com", "owner").await,
            StatusCode::UNPROCESSABLE_ENTITY,
            "nobody is invited as an owner",
        );
        refused(
            invite_as(&app, &owner, company_id, "y@example.com", "boss").await,
            StatusCode::UNPROCESSABLE_ENTITY,
            "role must be",
        );
        refused(
            invite_as(&app, &owner, company_id, "not-an-email", "viewer").await,
            StatusCode::UNPROCESSABLE_ENTITY,
            "email is not valid",
        );
        refused(
            invite_as(&app, &owner, company_id, "X@example.com", "sales").await,
            StatusCode::CONFLICT,
            "already has an open invitation",
        );
        refused(
            invite_as(&app, &owner, company_id, "dona@example.com", "sales").await,
            StatusCode::CONFLICT,
            "already a member",
        );
        let typo = send(
            &app,
            json_request(
                "POST",
                &invitations_uri(company_id),
                Some(&owner),
                json!({"email": "z@example.com", "role": "viewer", "roles": []}),
            ),
        )
        .await;
        assert!(typo.status.is_client_error());
    }

    #[tokio::test]
    async fn a_withdrawn_invitation_stops_working_and_only_its_company_can_withdraw_it() {
        let (app, _, _) = app_with_mailbox();
        let (ana, company_id) = sign_up(&app, "ana@example.com").await;
        let (outsider, outsider_company) = sign_up(&app, "fora@example.com").await;
        let (bia, _) = sign_up(&app, "bia@example.com").await;

        let invited = invite_as(&app, &ana, company_id, "bia@example.com", "viewer").await;
        let id = invited.body["id"].as_str().unwrap().to_owned();
        let token = token_of(&invited.body["link"]);
        let one = format!("{}/{id}", invitations_uri(company_id));

        // Another company's owner cannot see or withdraw it: it is not found for them.
        assert_eq!(
            send(
                &app,
                json_request("DELETE", &one, Some(&outsider), Value::Null)
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
        let theirs = format!("{}/{id}", invitations_uri(outsider_company));
        assert_eq!(
            send(
                &app,
                json_request("DELETE", &theirs, Some(&outsider), Value::Null)
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(
                &app,
                json_request(
                    "DELETE",
                    &format!("{}/not-an-id", invitations_uri(company_id)),
                    Some(&ana),
                    Value::Null
                )
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );

        assert_eq!(
            send(&app, json_request("DELETE", &one, Some(&ana), Value::Null))
                .await
                .status,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            send(&app, json_request("DELETE", &one, Some(&ana), Value::Null))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&format!("/invitations/{token}"), None))
                .await
                .status,
            StatusCode::GONE
        );
        let accept = json_request(
            "POST",
            &format!("/invitations/{token}/accept"),
            Some(&bia),
            Value::Null,
        );
        assert_eq!(send(&app, accept).await.status, StatusCode::GONE);
        // Bia is not a member: a withdrawn link gave her nothing.
        let members = send(
            &app,
            get_request(&format!("/companies/{company_id}/members"), Some(&ana)),
        )
        .await;
        assert_eq!(members.body.as_array().unwrap().len(), 1);
    }

    async fn register_with(app: &Router, body: Value) -> Reply {
        send(app, json_request("POST", "/auth/register", None, body)).await
    }

    #[tokio::test]
    async fn signing_up_through_an_invitation_joins_the_company_with_no_company_of_its_own() {
        let (app, _, _) = app_with_mailbox();
        let (ana, company_id) = sign_up(&app, "ana@example.com").await;
        let invited = invite_as(&app, &ana, company_id, "bia@example.com", "sales").await;
        let token = token_of(&invited.body["link"]);

        let reply = register_with(
            &app,
            json!({ "name": "Bia", "email": "Bia@Example.com", "password": "correct horse", "invitation": token }),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
        // Signed in at once, a member of the invited company in its role, and of no other.
        let companies = reply.body["companies"].as_array().unwrap();
        assert_eq!(companies.len(), 1);
        assert_eq!(companies[0]["id"], company_id.to_string());
        assert_eq!(companies[0]["role"], "sales");
        assert_eq!(reply.body["user"]["email"], "bia@example.com");
        let cookie = reply
            .set_cookie
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let listed = send(&app, get_request(&parties_uri(company_id), Some(&cookie))).await;
        assert_eq!(listed.status, StatusCode::OK);
        let me = send(&app, get_request("/auth/me", Some(&cookie))).await;
        assert_eq!(me.body["companies"].as_array().unwrap().len(), 1);
        // She can sign in again with the password she chose, and the invitation is used up.
        let login = send(
            &app,
            json_request(
                "POST",
                "/auth/login",
                None,
                json!({ "email": "bia@example.com", "password": "correct horse" }),
            ),
        )
        .await;
        assert_eq!(login.status, StatusCode::OK);
        let again = register_with(
            &app,
            json!({ "name": "Cris", "email": "bia@example.com", "password": "correct horse", "invitation": token }),
        )
        .await;
        assert_eq!(again.status, StatusCode::GONE);
        let members = send(
            &app,
            get_request(&format!("/companies/{company_id}/members"), Some(&ana)),
        )
        .await;
        assert_eq!(members.body.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_sign_up_through_an_invitation_that_is_refused_leaves_no_account_and_the_invitation_open()
     {
        let (app, _, _) = app_with_mailbox();
        let (ana, company_id) = sign_up(&app, "ana@example.com").await;
        let (_, _) = sign_up(&app, "ja-tem-conta@example.com").await;
        let token_for = |email: &str| {
            let app = app.clone();
            let ana = ana.clone();
            let email = email.to_owned();
            async move {
                let invited = invite_as(&app, &ana, company_id, &email, "viewer").await;
                token_of(&invited.body["link"])
            }
        };
        let bia = token_for("bia@example.com").await;
        let signs_up = |email: &str, password: &str, token: &str| {
            register_with(
                &app,
                json!({ "name": "Alguém", "email": email, "password": password, "invitation": token }),
            )
        };
        let preview = |token: &str| send(&app, get_request(&format!("/invitations/{token}"), None));

        // Another address than the one invited: refused, and no account for it.
        let wrong = signs_up("outra@example.com", "correct horse", &bia).await;
        assert_eq!(wrong.status, StatusCode::FORBIDDEN);
        assert!(
            wrong.body["error"]
                .as_str()
                .unwrap()
                .contains("another e-mail address")
        );
        let login = send(
            &app,
            json_request(
                "POST",
                "/auth/login",
                None,
                json!({ "email": "outra@example.com", "password": "correct horse" }),
            ),
        )
        .await;
        assert_eq!(login.status, StatusCode::UNAUTHORIZED);
        // A weak password: refused before anything is stored, and the invitation is still there.
        let weak = signs_up("bia@example.com", "short", &bia).await;
        assert_eq!(weak.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(preview(&bia).await.status, StatusCode::OK);
        // A link that does not exist.
        assert_eq!(
            signs_up("bia@example.com", "correct horse", "not-a-token")
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        // An address that already has an account is told so, and the invitation stays open.
        let taken = token_for("ja-tem-conta@example.com").await;
        let duplicate = signs_up("ja-tem-conta@example.com", "correct horse", &taken).await;
        assert_eq!(duplicate.status, StatusCode::CONFLICT);
        assert!(
            duplicate.body["error"]
                .as_str()
                .unwrap()
                .contains("already exists")
        );
        assert_eq!(preview(&taken).await.status, StatusCode::OK);
        // A withdrawn one.
        let id = send(&app, get_request(&invitations_uri(company_id), Some(&ana)))
            .await
            .body[0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let revoke = json_request(
            "DELETE",
            &format!("{}/{id}", invitations_uri(company_id)),
            Some(&ana),
            Value::Null,
        );
        assert_eq!(send(&app, revoke).await.status, StatusCode::NO_CONTENT);
        assert_eq!(
            signs_up("bia@example.com", "correct horse", &bia)
                .await
                .status,
            StatusCode::GONE
        );
        // None of this made Bia a member.
        let members = send(
            &app,
            get_request(&format!("/companies/{company_id}/members"), Some(&ana)),
        )
        .await;
        assert_eq!(members.body.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_ordinary_sign_up_still_needs_a_company_name_and_an_invited_one_refuses_one() {
        let (app, _, _) = app_with_mailbox();
        let missing = register_with(
            &app,
            json!({ "name": "Ana", "email": "ana@example.com", "password": "correct horse" }),
        )
        .await;
        assert_eq!(missing.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            missing.body["error"]
                .as_str()
                .unwrap()
                .contains("company_name is required")
        );

        let (ana, company_id) = sign_up(&app, "dona@example.com").await;
        let invited = invite_as(&app, &ana, company_id, "bia@example.com", "viewer").await;
        let both = register_with(
            &app,
            json!({
                "name": "Bia", "email": "bia@example.com", "password": "correct horse",
                "company_name": "Minha empresa", "invitation": token_of(&invited.body["link"])
            }),
        )
        .await;
        assert_eq!(both.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            both.body["error"]
                .as_str()
                .unwrap()
                .contains("company_name is not used")
        );
    }

    async fn register_party(app: &Router, document: Option<&str>) -> Reply {
        let (cookie, company_id) = sign_up(app, "ana@example.com").await;
        let mut body = party_body();
        body["document"] = document.map_or(Value::Null, Value::from);
        send(
            app,
            json_request("POST", &parties_uri(company_id), Some(&cookie), body),
        )
        .await
    }

    #[tokio::test]
    async fn a_known_cnpj_is_verified_against_the_registry() {
        let reply = register_party(&app(), Some("11.222.333/0001-81")).await;
        assert_eq!(reply.status, StatusCode::CREATED);
        assert_eq!(
            reply.body["document_check"],
            json!({
                "status": "verified",
                "legal_name": "PADARIA CENTRAL LTDA",
                "situation": "ATIVA",
                "needs_attention": false
            })
        );
    }

    #[tokio::test]
    async fn an_inactive_company_is_accepted_with_a_warning() {
        let reply = register_party(&app(), Some("12.ABC.345/01DE-35")).await;
        assert_eq!(reply.status, StatusCode::CREATED);
        assert_eq!(reply.body["document"], "12ABC34501DE35");
        assert_eq!(reply.body["document_check"]["situation"], "BAIXADA");
        assert_eq!(reply.body["document_check"]["needs_attention"], true);
    }

    #[tokio::test]
    async fn a_cnpj_missing_from_the_registry_is_accepted_with_a_warning() {
        // Valid check digits, but unknown to the registry, whose data lags: a new company must
        // still be registrable.
        let reply = register_party(&app(), Some("00.000.000/0001-91")).await;
        assert_eq!(reply.status, StatusCode::CREATED);
        assert_eq!(reply.body["document"], "00000000000191");
        assert_eq!(reply.body["document_check"]["status"], "not_found");
        assert_eq!(reply.body["document_check"]["needs_attention"], true);
    }

    #[tokio::test]
    async fn an_unreachable_registry_does_not_stop_data_entry() {
        let app = app_with_registry(InMemoryCnpjRegistry::unreachable());
        let reply = register_party(&app, Some("11.222.333/0001-81")).await;
        assert_eq!(reply.status, StatusCode::CREATED);
        assert_eq!(reply.body["document_check"]["status"], "unverified");
        assert_eq!(reply.body["document_check"]["needs_attention"], true);
    }

    /// Signs up, registers a party with the given document and returns the pieces a follow-up
    /// request needs: the session cookie, the company id and the party id.
    async fn party_of_a_new_user(
        app: &Router,
        email: &str,
        document: &str,
    ) -> (String, Uuid, Uuid) {
        let (cookie, company_id) = sign_up(app, email).await;
        let mut body = party_body();
        body["document"] = Value::from(document);
        let created = send(
            app,
            json_request("POST", &parties_uri(company_id), Some(&cookie), body),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED);
        let party_id = created.body["id"].as_str().unwrap().parse().unwrap();
        (cookie, company_id, party_id)
    }

    fn document_check_uri(company_id: Uuid, party_id: Uuid) -> String {
        format!("{}/{party_id}/document-check", parties_uri(company_id))
    }

    #[tokio::test]
    async fn an_unverified_party_can_be_checked_again() {
        let registry = InMemoryCnpjRegistry::unreachable();
        let app = app_with_registry(registry.clone());
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;
        let uri = document_check_uri(company_id, party_id);

        // The registry is still down: the answer is the same, not an error.
        let still_down = send(&app, json_request("POST", &uri, Some(&cookie), Value::Null)).await;
        assert_eq!(still_down.status, StatusCode::OK);
        assert_eq!(still_down.body["document_check"]["status"], "unverified");

        registry.recover();
        registry.add_company("11222333000181", "PADARIA CENTRAL LTDA", "ATIVA");
        let verified = send(&app, json_request("POST", &uri, Some(&cookie), Value::Null)).await;
        assert_eq!(verified.status, StatusCode::OK);
        assert_eq!(verified.body["document_check"]["status"], "verified");
        assert_eq!(verified.body["document_check"]["situation"], "ATIVA");

        // The new answer was stored: the list shows it.
        let listed = send(&app, get_request(&parties_uri(company_id), Some(&cookie))).await;
        assert_eq!(
            listed.body["items"][0]["document_check"]["status"],
            "verified"
        );
    }

    #[tokio::test]
    async fn checking_again_needs_a_session_and_the_partys_own_company() {
        let registry = InMemoryCnpjRegistry::unreachable();
        let app = app_with_registry(registry);
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;
        let uri = document_check_uri(company_id, party_id);

        let anonymous = send(&app, json_request("POST", &uri, None, Value::Null)).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

        // Another user cannot reach a company they are not a member of...
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;
        let foreign = send(
            &app,
            json_request("POST", &uri, Some(&stranger_cookie), Value::Null),
        )
        .await;
        assert_eq!(foreign.status, StatusCode::NOT_FOUND);

        // ...nor reach a party through a company of their own.
        let through_own_company = send(
            &app,
            json_request(
                "POST",
                &document_check_uri(stranger_company, party_id),
                Some(&stranger_cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(through_own_company.status, StatusCode::NOT_FOUND);

        let unknown_party = send(
            &app,
            json_request(
                "POST",
                &document_check_uri(company_id, Uuid::new_v4()),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(unknown_party.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn checking_again_too_fast_is_answered_with_429() {
        // The default interval: a second re-check right away is refused.
        let app = app_with_parties(PartyService::new(
            InMemoryPartyRepository::default(),
            InMemoryCnpjRegistry::unreachable(),
        ));
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;
        let uri = document_check_uri(company_id, party_id);

        let first = send(&app, json_request("POST", &uri, Some(&cookie), Value::Null)).await;
        assert_eq!(first.status, StatusCode::OK);
        let second = send(&app, json_request("POST", &uri, Some(&cookie), Value::Null)).await;
        assert_eq!(second.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            second.body["error"],
            "this CNPJ was checked a moment ago, try again in a few seconds"
        );
    }

    #[tokio::test]
    async fn checking_again_with_the_lookup_off_is_a_409() {
        let registry: Option<InMemoryCnpjRegistry> = None;
        let app = router::<NoLookupPorts>(
            Services {
                bank_sync: standalone_bank_sync(),
                webhook_subscriptions: standalone_hooks(),
                api_keys: standalone_api_keys(),
                connectors: InMemoryConnectorRepository::default(),
                sale_claims: InMemorySaleClaimRepository::default(),
                parties: PartyService::new(InMemoryPartyRepository::default(), registry),
                products: ProductService::new(InMemoryProductRepository::default()),
                cost_centers: CostCenterService::new(InMemoryCostCenterRepository::default()),
                categories: CategoryService::new(InMemoryCategoryRepository::default()),
                bank_accounts: BankAccountService::new(InMemoryBankAccountRepository::default()),
                payment_methods: PaymentMethodService::new(
                    InMemoryPaymentMethodRepository::default(),
                ),
                payment_terms: PaymentTermService::new(InMemoryPaymentTermRepository::default()),
                entries: standalone_entries(),
                attachments: standalone_attachments(),
                charges: standalone_charges(),
                webhooks: standalone_webhooks(),
                payment_pages: standalone_payment_pages(),
                dunning: standalone_dunning(),
                collection: standalone_collection(),
                transfers: standalone_transfers(),
                recurrences: standalone_recurrences(),
                quotes: standalone_quotes(),
                sales: standalone_sales(),
                purchases: standalone_purchases(),
                stock: standalone_stock(),
                counts: standalone_counts(),
                contracts: ContractService::new(
                    InMemoryContractRepository::default(),
                    standalone_sales(),
                ),
                commissions: standalone_commissions(),
                price_lists: standalone_price_lists(),
                sales_reports: SalesReportService::new(InMemorySaleRepository::default()),
                reconciliation: standalone_reconciliation(),
                balances: standalone_balances(),
                period_locks: PeriodLockService::new(locks()),
                exports: standalone_exports(),
                packages: standalone_packages(),
                fiscal_documents: standalone_fiscal_documents(),
                fiscal_settings: FiscalSettingsService::new(
                    InMemoryFiscalSettingsRepository::default(),
                ),
                gateway_settings: test_gateway_settings(),
                dre: DreService::new(
                    InMemoryFinancialEntryRepository::default(),
                    InMemoryCategoryRepository::default(),
                ),
                postal_codes: PostalCodeService::new(None),
                members: standalone_members(),
                documents: standalone_documents(),
                auth: auth_service(),
            },
            ApiConfig::default(),
        );
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;
        let reply = send(
            &app,
            json_request(
                "POST",
                &document_check_uri(company_id, party_id),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CONFLICT);
        assert_eq!(
            reply.body["error"],
            "CNPJ lookup is switched off on this installation"
        );
    }

    #[tokio::test]
    async fn a_malformed_party_id_gets_the_json_error_like_any_other_miss() {
        let app = app();
        let (cookie, company_id, _) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;
        let uri = format!("{}/not-a-uuid/document-check", parties_uri(company_id));
        let reply = send(&app, json_request("POST", &uri, Some(&cookie), Value::Null)).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
        assert_eq!(reply.body["error"], "party not found");
    }

    fn history_uri(company_id: Uuid, party_id: impl std::fmt::Display) -> String {
        format!("{}/{party_id}/history", parties_uri(company_id))
    }

    #[tokio::test]
    async fn the_history_names_who_registered_the_party_and_what_they_set() {
        let app = app();
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;

        let reply = send(
            &app,
            get_request(&history_uri(company_id, party_id), Some(&cookie)),
        )
        .await;
        assert_eq!(reply.status, StatusCode::OK);
        let entries = reply.body.as_array().unwrap();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry["action"], "created");
        // The name is the one the user signed up with, taken from the session.
        assert_eq!(entry["actor"]["kind"], "user");
        assert_eq!(entry["actor"]["name"], "Ana");
        assert!(entry["actor"]["id"].is_string());
        assert!(entry["at"].as_i64().unwrap() > 1_700_000_000_000);
        let fields: Vec<&str> = entry["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|change| change["field"].as_str().unwrap())
            .collect();
        assert!(fields.contains(&"name") && fields.contains(&"document"));
        assert_eq!(entry["changes"][0]["from"], Value::Null);
        assert_eq!(entry["changes"][0]["to"], "Padaria Central");
    }

    #[tokio::test]
    async fn a_recheck_shows_up_in_the_history_under_whoever_made_it() {
        let registry = InMemoryCnpjRegistry::unreachable();
        let app = app_with_registry(registry.clone());
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;

        registry.recover();
        registry.add_company("11222333000181", "PADARIA CENTRAL LTDA", "ATIVA");
        let checked = send(
            &app,
            json_request(
                "POST",
                &document_check_uri(company_id, party_id),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(checked.status, StatusCode::OK);

        let reply = send(
            &app,
            get_request(&history_uri(company_id, party_id), Some(&cookie)),
        )
        .await;
        let entries = reply.body.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1]["action"], "updated");
        assert_eq!(entries[1]["changes"][0]["field"], "document_check");
        assert_eq!(entries[1]["changes"][0]["from"], "unverified");
        assert_eq!(entries[1]["changes"][0]["to"], "verified");
        // Oldest first, and no installation-wide counter in the response.
        assert_eq!(entries[0]["action"], "created");
        assert!(entries[0].get("sequence").is_none());
    }

    #[tokio::test]
    async fn the_history_needs_a_session_and_the_partys_own_company() {
        let app = app();
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "11.222.333/0001-81").await;
        let uri = history_uri(company_id, party_id);

        let anonymous = send(&app, get_request(&uri, None)).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);

        // Another user's company, and another company of the same user, both look like a miss.
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;
        let foreign = send(&app, get_request(&uri, Some(&stranger_cookie))).await;
        assert_eq!(foreign.status, StatusCode::NOT_FOUND);
        let through_own_company = send(
            &app,
            get_request(
                &history_uri(stranger_company, party_id),
                Some(&stranger_cookie),
            ),
        )
        .await;
        assert_eq!(through_own_company.status, StatusCode::NOT_FOUND);
        assert_eq!(through_own_company.body["error"], "party not found");

        let malformed = send(
            &app,
            get_request(&history_uri(company_id, "not-a-uuid"), Some(&cookie)),
        )
        .await;
        assert_eq!(malformed.status, StatusCode::NOT_FOUND);
        assert_eq!(malformed.body["error"], "party not found");
    }

    #[tokio::test]
    async fn a_cpf_party_has_nothing_to_check() {
        let app = app();
        let (cookie, company_id, party_id) =
            party_of_a_new_user(&app, "ana@example.com", "529.982.247-25").await;
        let reply = send(
            &app,
            json_request(
                "POST",
                &document_check_uri(company_id, party_id),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(reply.body["error"], "the party has no CNPJ to verify");
    }

    #[tokio::test]
    async fn a_cpf_is_validated_by_its_digits_only() {
        let app = app_with_registry(InMemoryCnpjRegistry::unreachable());
        let reply = register_party(&app, Some("529.982.247-25")).await;
        assert_eq!(reply.status, StatusCode::CREATED);
        assert_eq!(reply.body["document_check"]["status"], "not_applicable");
        assert_eq!(reply.body["document_check"]["needs_attention"], false);
    }

    #[tokio::test]
    async fn lists_parties_page_by_page() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = parties_uri(company_id);
        for name in ["Carla", "Ana", "Bruno"] {
            let body = json!({ "name": name, "is_customer": true, "is_supplier": false });
            send(&app, json_request("POST", &uri, Some(&cookie), body)).await;
        }

        let first = send(&app, get_request(&format!("{uri}?limit=2"), Some(&cookie))).await;
        let names = |reply: &Reply| -> Vec<String> {
            reply.body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["name"].as_str().unwrap().to_owned())
                .collect()
        };
        assert_eq!(names(&first), ["Ana", "Bruno"]);

        // Cursors contain `|` and spaces in general, so clients must percent-encode them.
        let cursor = first.body["next_cursor"]
            .as_str()
            .unwrap()
            .replace('|', "%7C");
        let second = send(
            &app,
            get_request(&format!("{uri}?limit=2&after={cursor}"), Some(&cookie)),
        )
        .await;
        assert_eq!(names(&second), ["Carla"]);
        assert!(second.body["next_cursor"].is_null());

        let bad = send(
            &app,
            get_request(&format!("{uri}?after=garbage"), Some(&cookie)),
        )
        .await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn filters_and_sorts_the_list_from_query_parameters() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = parties_uri(company_id);
        for (name, is_customer, is_supplier) in [
            ("Carla", true, false),
            ("Ana", true, true),
            ("Bruno", false, true),
        ] {
            let body =
                json!({ "name": name, "is_customer": is_customer, "is_supplier": is_supplier });
            send(&app, json_request("POST", &uri, Some(&cookie), body)).await;
        }
        let names = |reply: &Reply| -> Vec<String> {
            reply.body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["name"].as_str().unwrap().to_owned())
                .collect()
        };
        let list = |query: &str| {
            let request = get_request(&format!("{uri}?{query}"), Some(&cookie));
            send(&app, request)
        };

        assert_eq!(names(&list("sort=-name").await), ["Carla", "Bruno", "Ana"]);
        assert_eq!(names(&list("kind=supplier").await), ["Ana", "Bruno"]);
        assert_eq!(names(&list("q=AR").await), ["Carla"]);

        // The cursor of a descending page resumes in descending order.
        let first = list("sort=-name&limit=2").await;
        assert_eq!(names(&first), ["Carla", "Bruno"]);
        let cursor = first.body["next_cursor"]
            .as_str()
            .unwrap()
            .replace('|', "%7C");
        let second = list(&format!("sort=-name&limit=2&after={cursor}")).await;
        assert_eq!(names(&second), ["Ana"]);

        for query in [
            "kind=both",
            "kind=",
            "sort=",
            "sort=email",
            "sort=name%20desc",
            &format!("q={}", "a".repeat(101)),
        ] {
            let reply = list(query).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{query}");
            assert!(reply.body["error"].is_string());
        }
    }

    #[tokio::test]
    async fn other_users_and_unknown_companies_look_the_same() {
        let app = app();
        let (_, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger_cookie, _) = sign_up(&app, "bia@example.com").await;

        let foreign = send(
            &app,
            get_request(&parties_uri(company_id), Some(&stranger_cookie)),
        )
        .await;
        let unknown = send(
            &app,
            get_request(&parties_uri(Uuid::new_v4()), Some(&stranger_cookie)),
        )
        .await;
        assert_eq!(foreign.status, StatusCode::NOT_FOUND);
        assert_eq!(foreign.status, unknown.status);
        assert_eq!(foreign.body, unknown.body);

        let write = send(
            &app,
            json_request(
                "POST",
                &parties_uri(company_id),
                Some(&stranger_cookie),
                party_body(),
            ),
        )
        .await;
        assert_eq!(write.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn invalid_party_is_unprocessable_and_duplicate_document_conflicts() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = parties_uri(company_id);

        let invalid = json!({ "name": "", "is_customer": true, "is_supplier": false });
        let reply = send(&app, json_request("POST", &uri, Some(&cookie), invalid)).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(reply.body["error"], "name must not be empty");

        send(
            &app,
            json_request("POST", &uri, Some(&cookie), party_body()),
        )
        .await;
        let duplicate = send(
            &app,
            json_request("POST", &uri, Some(&cookie), party_body()),
        )
        .await;
        assert_eq!(duplicate.status, StatusCode::CONFLICT);
    }

    fn products_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/products")
    }

    fn import_products_uri(company_id: Uuid, query: &str) -> String {
        format!("/companies/{company_id}/products/import{query}")
    }

    #[tokio::test]
    async fn imports_products_from_a_csv_file_with_brazilian_prices_kinds_and_a_default_kind() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let file = "Kind;Name;SKU;Unit;NCM;Sale_Price;Cost_Price;Minimum_Stock;Active\n\
                    produto;Pão francês;pao-001;kg;1905.90.90;12,50;7;10;sim\n\
                    ;Bolo de milho;;un;;18,9;;;\n\
                    serviço;Entrega;ENT-1;un;;5;;;não\n";
        let reply = send(
            &app,
            csv_request(&import_products_uri(company_id, ""), Some(&cookie), file),
        )
        .await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
        assert_eq!(reply.body["created"], 3);
        assert_eq!(reply.body["refused"], json!([]));

        let listed = send(&app, get_request(&products_uri(company_id), Some(&cookie))).await;
        let items = listed.body["items"].as_array().unwrap();
        let names: Vec<&str> = items.iter().map(|i| i["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["Bolo de milho", "Entrega", "Pão francês"]);
        // A comma is read as the decimal point, exactly: no float on the way.
        assert_eq!(items[2]["sale_price"], "12.5");
        assert_eq!(items[0]["sale_price"], "18.9");
        assert_eq!(items[2]["sku"], "PAO-001");
        // A blank kind is a product and a blank active is on; a service is what it says.
        assert_eq!(
            (items[0]["kind"].clone(), items[0]["active"].clone()),
            (json!("product"), json!(true))
        );
        assert_eq!(
            (items[1]["kind"].clone(), items[1]["active"].clone()),
            (json!("service"), json!(false))
        );
    }

    #[tokio::test]
    async fn a_product_file_with_bad_rows_writes_nothing_and_a_registered_sku_is_skipped() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = import_products_uri(company_id, "");
        let bad = "name,unit,sku,sale_price,kind,active\n\
                   Pão,kg,PAO-1,5,,\n\
                   Bolo,un,PAO-1,5,,\n\
                   Suco,,S-1,5,,\n\
                   Chá,un,C-1,5,bebida,\n\
                   Mate,un,M-1,5,,talvez\n\
                   Doce,un,D-1,\"1.234,56\",,\n\
                   Pudim,un,PU-1,1.234,,\n\
                   Entrega,un,E-1,5,service,\n";
        let reply = send(&app, csv_request(&uri, Some(&cookie), bad)).await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(reply.body["created"], 0);
        let rows: Vec<(u64, &str)> = reply.body["refused"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| (n["row"].as_u64().unwrap(), n["message"].as_str().unwrap()))
            .collect();
        assert_eq!(rows.len(), 6, "{rows:?}");
        assert_eq!(rows[0], (3, "the same SKU as row 2"));
        assert_eq!(
            rows[1],
            (4, "unit must have 1 to 6 letters or digits, like UN or KG")
        );
        assert_eq!(rows[2], (5, "kind must be product or service"));
        assert_eq!(rows[3], (6, "active must be sim or não"));
        assert_eq!(
            rows[4],
            (
                7,
                "sale price must be a decimal with up to 13 integer and 6 fractional digits, written like 10.50"
            )
        );
        assert_eq!(rows[5].0, 8);
        assert!(rows[5].1.contains("`1.234` is ambiguous"), "{}", rows[5].1);
        let listed = send(&app, get_request(&products_uri(company_id), Some(&cookie))).await;
        assert!(listed.body["items"].as_array().unwrap().is_empty());

        let good = "name,unit,sku\nPão,kg,PAO-1\nBolo,un,\n";
        assert_eq!(
            send(&app, csv_request(&uri, Some(&cookie), good))
                .await
                .body["created"],
            2
        );
        // A file with no `active` column makes every product active.
        let listed = send(&app, get_request(&products_uri(company_id), Some(&cookie))).await;
        assert!(
            listed.body["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item["active"] == true)
        );
        let again = send(&app, csv_request(&uri, Some(&cookie), good)).await;
        assert_eq!(again.body["created"], 1);
        assert_eq!(
            again.body["skipped"],
            json!([{ "row": 2, "message": "a product with this SKU already exists" }])
        );
    }

    #[tokio::test]
    async fn a_product_file_needs_its_required_columns_and_a_session() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        let uri = import_products_uri(company_id, "");
        let reply = send(
            &app,
            csv_request(&uri, Some(&cookie), "name,sku\nPão,P-1\n"),
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            reply.body["error"]
                .as_str()
                .unwrap()
                .contains("no `unit` column")
        );
        let file = "name,unit\nPão,kg\n";
        assert_eq!(
            send(&app, csv_request(&uri, None, file)).await.status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(&app, csv_request(&uri, Some(&stranger), file))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        let dry = send(
            &app,
            csv_request(
                &import_products_uri(company_id, "?dry_run=true"),
                Some(&cookie),
                file,
            ),
        )
        .await;
        assert_eq!(
            (dry.body["valid"].clone(), dry.body["created"].clone()),
            (json!(1), json!(0))
        );
    }

    fn product_body() -> Value {
        json!({
            "kind": "product",
            "name": "Pão francês",
            "sku": "pao-001",
            "unit": "kg",
            "ncm": "1905.90.90",
            "sale_price": "12.50",
            "cost_price": "7",
            "minimum_stock": "10.000",
            "active": true
        })
    }

    async fn post_product(app: &Router, cookie: &str, company_id: Uuid, body: Value) -> Reply {
        let request = json_request("POST", &products_uri(company_id), Some(cookie), body);
        send(app, request).await
    }

    #[tokio::test]
    async fn registers_changes_and_lists_products_with_their_history() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;

        let created = post_product(&app, &cookie, company_id, product_body()).await;
        assert_eq!(created.status, StatusCode::CREATED);
        assert_eq!(created.body["sku"], "PAO-001");
        assert_eq!(created.body["unit"], "KG");
        assert_eq!(created.body["ncm"], "19059090");
        // Decimals come back as strings, without trailing zeros.
        assert_eq!(created.body["sale_price"], "12.5");
        assert_eq!(created.body["minimum_stock"], "10");
        assert_eq!(created.body["active"], true);
        let id = created.body["id"].as_str().unwrap().to_owned();

        let mut changed = product_body();
        changed["sale_price"] = json!("15");
        changed["active"] = json!(false);
        let uri = format!("{}/{id}", products_uri(company_id));
        let updated = send(
            &app,
            json_request("PUT", &uri, Some(&cookie), changed.clone()),
        )
        .await;
        assert_eq!(updated.status, StatusCode::OK);
        assert_eq!(updated.body["sale_price"], "15");
        assert_eq!(updated.body["active"], false);

        // The same body again changes nothing and adds nothing to the history.
        let again = send(&app, json_request("PUT", &uri, Some(&cookie), changed)).await;
        assert_eq!(again.status, StatusCode::OK);

        let listed = send(&app, get_request(&products_uri(company_id), Some(&cookie))).await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
        let inactive = format!(
            "{}?active=false&kind=product&q=PAO",
            products_uri(company_id)
        );
        let listed = send(&app, get_request(&inactive, Some(&cookie))).await;
        assert_eq!(listed.body["items"][0]["name"], "Pão francês");
        let active = format!("{}?active=true", products_uri(company_id));
        let listed = send(&app, get_request(&active, Some(&cookie))).await;
        assert!(listed.body["items"].as_array().unwrap().is_empty());

        let history = send(&app, get_request(&format!("{uri}/history"), Some(&cookie))).await;
        let entries = history.body.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["action"], "created");
        assert_eq!(entries[0]["actor"]["name"], "Ana");
        assert_eq!(entries[1]["action"], "updated");
        assert_eq!(
            entries[1]["changes"],
            json!([
                { "field": "sale_price", "from": "12.5", "to": "15" },
                { "field": "active", "from": "true", "to": "false" },
            ])
        );
    }

    #[tokio::test]
    async fn amounts_are_decimal_strings_and_every_refusal_is_json() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let with = |field: &str, value: Value| {
            let mut body = product_body();
            body[field] = value;
            body
        };

        // A JSON number would lose precision on the way: refused, in the API's own error shape.
        for body in [
            with("sale_price", json!(10.5)),
            with("kind", json!("gadget")),
            with("surprise", json!(1)),
            with("sale_price", json!({})),
            json!({ "kind": "product" }),
        ] {
            let reply = post_product(&app, &cookie, company_id, body.clone()).await;
            assert!(reply.status.is_client_error(), "{body}");
            assert!(reply.body["error"].is_string(), "{body}");
        }

        // The domain's own rules are 422 with a message that names the field.
        let comma = post_product(
            &app,
            &cookie,
            company_id,
            with("sale_price", json!("10,50")),
        )
        .await;
        assert_eq!(comma.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            comma.body["error"]
                .as_str()
                .unwrap()
                .starts_with("sale price must be")
        );
        for (field, value) in [
            ("cost_price", json!("-1")),
            ("minimum_stock", json!("1e3")),
            ("unit", json!("quilogramas")),
            ("ncm", json!("123")),
            ("sku", json!("a b")),
            ("name", json!("  ")),
        ] {
            let reply = post_product(&app, &cookie, company_id, with(field, value)).await;
            assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{field}");
        }
        let service = json!({
            "kind": "service", "name": "Entrega", "unit": "UN", "ncm": "19059090"
        });
        let reply = post_product(&app, &cookie, company_id, service).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn a_sku_is_unique_within_a_company_and_products_stay_private() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;

        let created = post_product(&app, &cookie, company_id, product_body()).await;
        let id = created.body["id"].as_str().unwrap().to_owned();
        let duplicate = post_product(&app, &cookie, company_id, product_body()).await;
        assert_eq!(duplicate.status, StatusCode::CONFLICT);
        // Another company may use the same SKU.
        let own = post_product(&app, &stranger_cookie, stranger_company, product_body()).await;
        assert_eq!(own.status, StatusCode::CREATED);

        // Neither the product nor its trail can be reached through another company's route...
        let foreign_uri = format!("{}/{id}", products_uri(stranger_company));
        let put = json_request("PUT", &foreign_uri, Some(&stranger_cookie), product_body());
        assert_eq!(send(&app, put).await.status, StatusCode::NOT_FOUND);
        let history = get_request(&format!("{foreign_uri}/history"), Some(&stranger_cookie));
        assert_eq!(send(&app, history).await.status, StatusCode::NOT_FOUND);
        // ...nor through a company the caller does not belong to.
        let uri = products_uri(company_id);
        let list = send(&app, get_request(&uri, Some(&stranger_cookie))).await;
        assert_eq!(list.status, StatusCode::NOT_FOUND);
        let unknown = format!("{uri}/not-a-uuid");
        let put = json_request("PUT", &unknown, Some(&cookie), product_body());
        let reply = send(&app, put).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
        assert_eq!(reply.body["error"], "product not found");

        let anonymous = send(&app, get_request(&uri, None)).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn product_list_parameters_are_checked_and_paged() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        for name in ["Carla", "Ana", "Bruno"] {
            let mut body = product_body();
            body["name"] = json!(name);
            body["sku"] = Value::Null;
            post_product(&app, &cookie, company_id, body).await;
        }
        let uri = products_uri(company_id);
        let first = send(
            &app,
            get_request(&format!("{uri}?limit=2&sort=-name"), Some(&cookie)),
        )
        .await;
        let names: Vec<&str> = first.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Carla", "Bruno"]);
        let cursor = first.body["next_cursor"]
            .as_str()
            .unwrap()
            .replace('|', "%7C");
        let second = send(
            &app,
            get_request(
                &format!("{uri}?limit=2&sort=-name&after={cursor}"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(second.body["items"][0]["name"], "Ana");

        for query in [
            "kind=both",
            "kind=",
            "active=yes",
            "active=",
            "sort=sku",
            "after=garbage",
        ] {
            let reply = send(&app, get_request(&format!("{uri}?{query}"), Some(&cookie))).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{query}");
            assert!(reply.body["error"].is_string(), "{query}");
        }
    }

    #[tokio::test]
    async fn replacing_must_say_whether_the_product_is_active() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let mut inactive = product_body();
        inactive["active"] = json!(false);
        let created = post_product(&app, &cookie, company_id, inactive).await;
        assert_eq!(created.body["active"], false);
        let id = created.body["id"].as_str().unwrap();

        // Registering may leave it out (active), replacing may not: it would switch the product on.
        let uri = format!("{}/{id}", products_uri(company_id));
        let mut without_active = product_body();
        without_active.as_object_mut().unwrap().remove("active");
        let reply = send(
            &app,
            json_request("PUT", &uri, Some(&cookie), without_active.clone()),
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            reply.body["error"]
                .as_str()
                .unwrap()
                .contains("active is required")
        );
        let listed = send(
            &app,
            get_request(
                &format!("{}?active=true", products_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert!(listed.body["items"].as_array().unwrap().is_empty());
        without_active["sku"] = json!("other");
        let bare = post_product(&app, &cookie, company_id, without_active).await;
        assert_eq!(bare.body["active"], true);
    }

    #[tokio::test]
    async fn malformed_list_parameters_and_overlong_names_are_json_refusals() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        for uri in [products_uri(company_id), parties_uri(company_id)] {
            let reply = send(
                &app,
                get_request(&format!("{uri}?limit=abc"), Some(&cookie)),
            )
            .await;
            assert!(reply.status.is_client_error(), "{uri}");
            assert!(reply.body["error"].is_string(), "{uri}");
        }
        let mut long = product_body();
        long["name"] = json!("ã".repeat(201));
        let reply = post_product(&app, &cookie, company_id, long).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            reply.body["error"]
                .as_str()
                .unwrap()
                .contains("200 characters")
        );
    }

    fn cost_centers_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/cost-centers")
    }

    async fn post_cost_center(app: &Router, cookie: &str, company_id: Uuid, body: Value) -> Reply {
        let request = json_request("POST", &cost_centers_uri(company_id), Some(cookie), body);
        send(app, request).await
    }

    #[tokio::test]
    async fn registers_changes_and_lists_cost_centers_with_their_history() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;

        let created = post_cost_center(
            &app,
            &cookie,
            company_id,
            json!({ "name": "  Loja Centro " }),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED);
        assert_eq!(created.body["name"], "Loja Centro");
        assert_eq!(created.body["active"], true);
        let id = created.body["id"].as_str().unwrap().to_owned();

        // The same name in another case is the same cost center.
        let duplicate =
            post_cost_center(&app, &cookie, company_id, json!({ "name": "LOJA centro" })).await;
        assert_eq!(duplicate.status, StatusCode::CONFLICT);

        let uri = format!("{}/{id}", cost_centers_uri(company_id));
        let changed = json!({ "name": "Loja Norte", "active": false });
        let updated = send(
            &app,
            json_request("PUT", &uri, Some(&cookie), changed.clone()),
        )
        .await;
        assert_eq!(updated.status, StatusCode::OK);
        assert_eq!(updated.body["active"], false);
        // The same body again changes nothing and adds nothing to the history.
        send(&app, json_request("PUT", &uri, Some(&cookie), changed)).await;

        let inactive = format!("{}?active=false&q=NORTE", cost_centers_uri(company_id));
        let listed = send(&app, get_request(&inactive, Some(&cookie))).await;
        assert_eq!(listed.body["items"][0]["name"], "Loja Norte");

        let history = send(&app, get_request(&format!("{uri}/history"), Some(&cookie))).await;
        let entries = history.body.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["action"], "created");
        assert_eq!(entries[0]["actor"]["name"], "Ana");
        assert_eq!(
            entries[1]["changes"],
            json!([
                { "field": "name", "from": "Loja Centro", "to": "Loja Norte" },
                { "field": "active", "from": "true", "to": "false" },
            ])
        );

        // Replacing must say whether it is active, or a deactivated one would come back.
        let without = json_request("PUT", &uri, Some(&cookie), json!({ "name": "Loja Norte" }));
        assert_eq!(
            send(&app, without).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn cost_centers_stay_private_and_refusals_are_json() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;
        let created = post_cost_center(&app, &cookie, company_id, json!({ "name": "Loja" })).await;
        let id = created.body["id"].as_str().unwrap().to_owned();

        // The same name is fine in another company; its routes cannot reach this one's.
        let own = post_cost_center(
            &app,
            &stranger_cookie,
            stranger_company,
            json!({ "name": "Loja" }),
        )
        .await;
        assert_eq!(own.status, StatusCode::CREATED);
        let foreign = format!("{}/{id}", cost_centers_uri(stranger_company));
        let put = json_request(
            "PUT",
            &foreign,
            Some(&stranger_cookie),
            json!({ "name": "X", "active": true }),
        );
        assert_eq!(send(&app, put).await.status, StatusCode::NOT_FOUND);
        let history = get_request(&format!("{foreign}/history"), Some(&stranger_cookie));
        assert_eq!(send(&app, history).await.status, StatusCode::NOT_FOUND);
        let list = get_request(&cost_centers_uri(company_id), Some(&stranger_cookie));
        assert_eq!(send(&app, list).await.status, StatusCode::NOT_FOUND);
        let anonymous = get_request(&cost_centers_uri(company_id), None);
        assert_eq!(send(&app, anonymous).await.status, StatusCode::UNAUTHORIZED);

        let malformed = format!("{}/not-a-uuid", cost_centers_uri(company_id));
        let put = json_request(
            "PUT",
            &malformed,
            Some(&cookie),
            json!({ "name": "X", "active": true }),
        );
        let reply = send(&app, put).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
        assert_eq!(reply.body["error"], "cost center not found");

        for body in [
            json!({}),
            json!({ "name": "X", "surprise": 1 }),
            json!({ "name": 5 }),
        ] {
            let reply = post_cost_center(&app, &cookie, company_id, body.clone()).await;
            assert!(reply.status.is_client_error(), "{body}");
            assert!(reply.body["error"].is_string(), "{body}");
        }
        let blank = post_cost_center(&app, &cookie, company_id, json!({ "name": "  " })).await;
        assert_eq!(blank.status, StatusCode::UNPROCESSABLE_ENTITY);
        let long = post_cost_center(
            &app,
            &cookie,
            company_id,
            json!({ "name": "x".repeat(201) }),
        )
        .await;
        assert_eq!(long.status, StatusCode::UNPROCESSABLE_ENTITY);

        let uri = cost_centers_uri(company_id);
        for query in [
            "active=yes",
            "active=",
            "sort=id",
            "limit=abc",
            "after=garbage",
        ] {
            let reply = send(&app, get_request(&format!("{uri}?{query}"), Some(&cookie))).await;
            assert!(reply.status.is_client_error(), "{query}");
            assert!(reply.body["error"].is_string(), "{query}");
        }
    }

    fn categories_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/categories")
    }

    async fn post_category(app: &Router, cookie: &str, company_id: Uuid, body: Value) -> Reply {
        let request = json_request("POST", &categories_uri(company_id), Some(cookie), body);
        send(app, request).await
    }

    #[tokio::test]
    async fn builds_a_tree_of_categories_whose_groups_come_from_the_root() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;

        let root = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Despesas", "dre_group": "operating_expenses", "cash_flow_group": "operating" }),
        )
        .await;
        assert_eq!(root.status, StatusCode::CREATED);
        assert_eq!(root.body["parent_id"], Value::Null);
        assert_eq!(root.body["level"], 1);
        let root_id = root.body["id"].as_str().unwrap().to_owned();

        let child = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Aluguel", "parent_id": root_id }),
        )
        .await;
        assert_eq!(child.status, StatusCode::CREATED);
        assert_eq!(child.body["parent_id"], root_id.as_str());
        assert_eq!(child.body["dre_group"], "operating_expenses");
        assert_eq!(child.body["level"], 2);
        let child_id = child.body["id"].as_str().unwrap().to_owned();

        // The groups of a subcategory are not its own to choose, and a root has to choose them.
        let with_groups = json!({ "name": "X", "parent_id": root_id, "dre_group": "costs", "cash_flow_group": "operating" });
        assert_eq!(
            post_category(&app, &cookie, company_id, with_groups)
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let bare_root = post_category(&app, &cookie, company_id, json!({ "name": "Y" })).await;
        assert_eq!(bare_root.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            bare_root.body["error"]
                .as_str()
                .unwrap()
                .contains("needs a DRE group")
        );
        let duplicate = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "ALUGUEL", "parent_id": root_id }),
        )
        .await;
        assert_eq!(duplicate.status, StatusCode::CONFLICT);

        // The whole tree, flat and in a fixed order; the active ones can be asked for alone.
        let listed = send(
            &app,
            get_request(&categories_uri(company_id), Some(&cookie)),
        )
        .await;
        let names: Vec<&str> = listed.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Aluguel", "Despesas"]);

        // Switched off from the bottom up; a body that moves it or changes its groups is refused.
        let uri = |id: &str| format!("{}/{id}", categories_uri(company_id));
        let off = |name: &str| json!({ "name": name, "active": false });
        let blocked = send(
            &app,
            json_request("PUT", &uri(&root_id), Some(&cookie), off("Despesas")),
        )
        .await;
        assert_eq!(blocked.status, StatusCode::CONFLICT);
        let leaf = send(
            &app,
            json_request("PUT", &uri(&child_id), Some(&cookie), off("Aluguel")),
        )
        .await;
        assert_eq!(leaf.status, StatusCode::OK);
        assert_eq!(leaf.body["active"], false);
        let moved = json!({ "name": "Aluguel", "active": true, "parent_id": root_id });
        assert!(
            send(
                &app,
                json_request("PUT", &uri(&child_id), Some(&cookie), moved)
            )
            .await
            .status
            .is_client_error()
        );
        let regrouped = json!({ "name": "Aluguel", "active": true, "dre_group": "costs" });
        assert!(
            send(
                &app,
                json_request("PUT", &uri(&child_id), Some(&cookie), regrouped)
            )
            .await
            .status
            .is_client_error()
        );
        let without_active = json_request(
            "PUT",
            &uri(&child_id),
            Some(&cookie),
            json!({ "name": "Aluguel" }),
        );
        assert_eq!(
            send(&app, without_active).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let active_only = format!("{}?active=true", categories_uri(company_id));
        let listed = send(&app, get_request(&active_only, Some(&cookie))).await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);

        // Under an inactive parent nothing active can go.
        let off_root = send(
            &app,
            json_request("PUT", &uri(&root_id), Some(&cookie), off("Despesas")),
        )
        .await;
        assert_eq!(off_root.status, StatusCode::OK);
        let under = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Luz", "parent_id": root_id }),
        )
        .await;
        assert_eq!(under.status, StatusCode::CONFLICT);
        let unknown = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Luz", "parent_id": Uuid::new_v4() }),
        )
        .await;
        assert_eq!(unknown.status, StatusCode::UNPROCESSABLE_ENTITY);

        let history = send(
            &app,
            get_request(&format!("{}/history", uri(&child_id)), Some(&cookie)),
        )
        .await;
        let entries = history.body.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[1]["changes"],
            json!([{ "field": "active", "from": "true", "to": "false" }])
        );
    }

    #[tokio::test]
    async fn the_default_template_is_applied_once_and_the_transfer_category_is_kept() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;
        let template = format!("{}/default-template", categories_uri(company_id));

        let applied = send(
            &app,
            json_request("POST", &template, Some(&cookie), Value::Null),
        )
        .await;
        assert_eq!(applied.status, StatusCode::CREATED);
        let items = applied.body["items"].as_array().unwrap();
        assert!(items.len() > 20);
        let transfers: Vec<&Value> = items
            .iter()
            .filter(|item| item["dre_group"] == "transfers")
            .collect();
        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0]["cash_flow_group"], "transfer");
        assert_eq!(transfers[0]["parent_id"], Value::Null);
        let again = send(
            &app,
            json_request("POST", &template, Some(&cookie), Value::Null),
        )
        .await;
        assert_eq!(again.status, StatusCode::CONFLICT);

        // It can be renamed but not switched off, and it cannot be duplicated.
        let uri = format!(
            "{}/{}",
            categories_uri(company_id),
            transfers[0]["id"].as_str().unwrap()
        );
        let off = json_request(
            "PUT",
            &uri,
            Some(&cookie),
            json!({ "name": "Transferências", "active": false }),
        );
        assert_eq!(
            send(&app, off).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let renamed = json_request(
            "PUT",
            &uri,
            Some(&cookie),
            json!({ "name": "Entre contas", "active": true }),
        );
        assert_eq!(send(&app, renamed).await.body["dre_group"], "transfers");
        let second = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Outra", "dre_group": "transfers", "cash_flow_group": "transfer" }),
        )
        .await;
        assert_eq!(second.status, StatusCode::CONFLICT);
        let mismatched = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Outra", "dre_group": "transfers", "cash_flow_group": "operating" }),
        )
        .await;
        assert_eq!(mismatched.status, StatusCode::UNPROCESSABLE_ENTITY);
        let unknown_group = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Outra", "dre_group": "profit", "cash_flow_group": "operating" }),
        )
        .await;
        assert_eq!(unknown_group.status, StatusCode::UNPROCESSABLE_ENTITY);

        // Another company sees none of it, cannot change it, and starts from nothing.
        let theirs = send(
            &app,
            get_request(&categories_uri(stranger_company), Some(&stranger_cookie)),
        )
        .await;
        assert!(theirs.body["items"].as_array().unwrap().is_empty());
        let foreign = json_request(
            "PUT",
            &format!(
                "{}/{}",
                categories_uri(stranger_company),
                transfers[0]["id"].as_str().unwrap()
            ),
            Some(&stranger_cookie),
            json!({ "name": "X", "active": true }),
        );
        assert_eq!(send(&app, foreign).await.status, StatusCode::NOT_FOUND);
        let not_theirs = send(
            &app,
            get_request(&categories_uri(company_id), Some(&stranger_cookie)),
        )
        .await;
        assert_eq!(not_theirs.status, StatusCode::NOT_FOUND);
        let anonymous = send(&app, json_request("POST", &template, None, Value::Null)).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
        let malformed = json_request(
            "PUT",
            &format!("{}/not-a-uuid", categories_uri(company_id)),
            Some(&cookie),
            json!({ "name": "X", "active": true }),
        );
        let reply = send(&app, malformed).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
        assert_eq!(reply.body["error"], "category not found");
        for query in [
            "active=yes",
            "active=",
            "active=maybe",
            "q=x",
            "sort=-name",
            "limit=1",
        ] {
            let uri = format!("{}?{query}", categories_uri(company_id));
            let reply = send(&app, get_request(&uri, Some(&cookie))).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{query}");
            assert!(reply.body["error"].is_string(), "{query}");
        }
    }

    fn bank_accounts_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/bank-accounts")
    }

    async fn post_to(app: &Router, uri: &str, cookie: &str, body: Value) -> Reply {
        send(app, json_request("POST", uri, Some(cookie), body)).await
    }

    #[tokio::test]
    async fn registers_and_changes_bank_accounts_stored_in_one_form() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = bank_accounts_uri(company_id);

        let created = post_to(
            &app,
            &uri,
            &cookie,
            json!({ "kind": "checking", "name": " Conta da loja ", "bank_code": "341", "branch": "12.34", "number": "56789-x" }),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED);
        assert_eq!(created.body["name"], "Conta da loja");
        assert_eq!(created.body["branch"], "1234");
        assert_eq!(created.body["number"], "56789X");
        assert_eq!(created.body["active"], true);
        let id = created.body["id"].as_str().unwrap().to_owned();

        // The same bank, branch and number is the same account, however it is written.
        let same = json!({ "kind": "checking", "name": "Outra", "bank_code": "341", "branch": "1234", "number": "56789 X" });
        assert_eq!(
            post_to(&app, &uri, &cookie, same).await.status,
            StatusCode::CONFLICT
        );
        // Under another kind it is another account: a savings account may carry the checking one's.
        let savings = json!({ "kind": "savings", "name": "Poupança", "bank_code": "341", "branch": "1234", "number": "56789 X" });
        assert_eq!(
            post_to(&app, &uri, &cookie, savings).await.status,
            StatusCode::CREATED
        );
        let same_name = json!({ "kind": "cash", "name": "CONTA DA LOJA" });
        assert_eq!(
            post_to(&app, &uri, &cookie, same_name).await.status,
            StatusCode::CONFLICT
        );
        // Cash takes no bank.
        let cash = json!({ "kind": "cash", "name": "Caixa", "number": "1" });
        assert_eq!(
            post_to(&app, &uri, &cookie, cash).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        // A card keeps its issuer and never a number: the audit trail would keep it for good.
        let card = json!({ "kind": "card", "name": "Visa", "bank_code": "341", "number": "4111111111111111" });
        let refused = post_to(&app, &uri, &cookie, card).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            refused.body["error"]
                .as_str()
                .unwrap()
                .contains("card number is not stored")
        );
        let card = json!({ "kind": "card", "name": "Visa", "bank_code": "341" });
        assert_eq!(
            post_to(&app, &uri, &cookie, card).await.status,
            StatusCode::CREATED
        );

        let account_uri = format!("{uri}/{id}");
        let changed = json!({ "kind": "checking", "name": "Conta da loja", "bank_code": "341", "branch": "1234", "number": "99999", "active": false });
        let updated = send(
            &app,
            json_request("PUT", &account_uri, Some(&cookie), changed.clone()),
        )
        .await;
        assert_eq!(updated.status, StatusCode::OK);
        assert_eq!(updated.body["number"], "99999");
        // The same body again changes nothing and adds nothing to the history.
        send(
            &app,
            json_request("PUT", &account_uri, Some(&cookie), changed),
        )
        .await;
        // The kind is fixed, and `active` has to be said when replacing.
        let other_kind = json!({ "kind": "savings", "name": "Conta da loja", "active": true });
        let reply = send(
            &app,
            json_request("PUT", &account_uri, Some(&cookie), other_kind),
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        let without_active = json!({ "kind": "checking", "name": "Conta da loja" });
        let reply = send(
            &app,
            json_request("PUT", &account_uri, Some(&cookie), without_active),
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);

        let found = send(
            &app,
            get_request(
                &format!("{uri}?kind=checking&active=false&q=99-9"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(found.body["items"][0]["name"], "Conta da loja");
        let history = send(
            &app,
            get_request(&format!("{account_uri}/history"), Some(&cookie)),
        )
        .await;
        let entries = history.body.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[1]["changes"],
            json!([
                { "field": "number", "from": "56789X", "to": "99999" },
                { "field": "active", "from": "true", "to": "false" },
            ])
        );
    }

    #[tokio::test]
    async fn bank_accounts_and_payment_methods_stay_private_and_refusals_are_json() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;
        for (path, body) in [
            (
                "bank-accounts",
                json!({ "kind": "checking", "name": "Conta" }),
            ),
            ("payment-methods", json!({ "kind": "pix", "name": "Pix" })),
        ] {
            let uri = format!("/companies/{company_id}/{path}");
            let created = post_to(&app, &uri, &cookie, body.clone()).await;
            assert_eq!(created.status, StatusCode::CREATED, "{path}");
            let id = created.body["id"].as_str().unwrap().to_owned();

            // Another company has its own, and cannot reach this one's.
            let theirs = format!("/companies/{stranger_company}/{path}");
            assert_eq!(
                post_to(&app, &theirs, &stranger_cookie, body.clone())
                    .await
                    .status,
                StatusCode::CREATED
            );
            let mut replacement = body.clone();
            replacement["active"] = json!(true);
            let put = json_request(
                "PUT",
                &format!("{theirs}/{id}"),
                Some(&stranger_cookie),
                replacement.clone(),
            );
            assert_eq!(
                send(&app, put).await.status,
                StatusCode::NOT_FOUND,
                "{path}"
            );
            let history = get_request(&format!("{theirs}/{id}/history"), Some(&stranger_cookie));
            assert_eq!(
                send(&app, history).await.status,
                StatusCode::NOT_FOUND,
                "{path}"
            );
            assert_eq!(
                send(&app, get_request(&uri, Some(&stranger_cookie)))
                    .await
                    .status,
                StatusCode::NOT_FOUND
            );
            assert_eq!(
                send(&app, get_request(&uri, None)).await.status,
                StatusCode::UNAUTHORIZED
            );

            let malformed = json_request(
                "PUT",
                &format!("{uri}/not-a-uuid"),
                Some(&cookie),
                replacement,
            );
            let reply = send(&app, malformed).await;
            assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
            assert!(reply.body["error"].as_str().unwrap().ends_with("not found"));

            // Every refusal has the API's own shape.
            for bad in [
                json!({}),
                json!({ "kind": "gadget", "name": "X" }),
                json!({ "kind": "pix", "name": "X", "surprise": 1 }),
                json!({ "kind": 5, "name": "X" }),
            ] {
                let reply = post_to(&app, &uri, &cookie, bad.clone()).await;
                assert!(reply.status.is_client_error(), "{path} {bad}");
                assert!(reply.body["error"].is_string(), "{path} {bad}");
            }
            let long = json!({ "kind": "pix", "name": "x".repeat(201) });
            assert_eq!(
                post_to(&app, &uri, &cookie, long).await.status,
                StatusCode::UNPROCESSABLE_ENTITY
            );
            for query in [
                "kind=gadget",
                "kind=",
                "active=yes",
                "sort=id",
                "limit=abc",
                "after=garbage",
            ] {
                let reply = send(&app, get_request(&format!("{uri}?{query}"), Some(&cookie))).await;
                assert!(reply.status.is_client_error(), "{path} {query}");
                assert!(reply.body["error"].is_string(), "{path} {query}");
            }
        }
    }

    #[tokio::test]
    async fn registers_changes_and_lists_payment_methods() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = format!("/companies/{company_id}/payment-methods");

        let created = post_to(
            &app,
            &uri,
            &cookie,
            json!({ "kind": "credit_card", "name": " Visa " }),
        )
        .await;
        assert_eq!(created.body["name"], "Visa");
        assert_eq!(created.body["kind"], "credit_card");
        assert_eq!(
            post_to(
                &app,
                &uri,
                &cookie,
                json!({ "kind": "pix", "name": "VISA" })
            )
            .await
            .status,
            StatusCode::CONFLICT
        );
        let id = created.body["id"].as_str().unwrap();

        // The kind may change with the rest, unlike an account's.
        let change = json!({ "kind": "debit_card", "name": "Visa débito", "active": false });
        let updated = send(
            &app,
            json_request("PUT", &format!("{uri}/{id}"), Some(&cookie), change),
        )
        .await;
        assert_eq!(updated.body["kind"], "debit_card");
        let listed = send(
            &app,
            get_request(
                &format!("{uri}?kind=debit_card&active=false&sort=-name"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(listed.body["items"][0]["name"], "Visa débito");
        let history = send(
            &app,
            get_request(&format!("{uri}/{id}/history"), Some(&cookie)),
        )
        .await;
        assert_eq!(history.body.as_array().unwrap().len(), 2);
    }

    fn entries_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/entries")
    }

    /// An app whose entries read the very registers its categories, cost centers and parties
    /// write to, which is what pointing an entry at a record needs.
    fn entry_app() -> Router {
        entry_app_with_mailbox().0
    }

    /// Like `entry_app`, and the mailbox the e-mails of its documents go to; its purchases and documents share a store.
    fn entry_app_with_mailbox() -> (Router, InMemoryEmailSender) {
        entry_app_over(InMemoryIdentityRepository::default())
    }

    /// Like `entry_app`, and the identities it signs people in from, so a test can give a member a role.
    fn entry_app_with_identities() -> (Router, InMemoryIdentityRepository) {
        let identities = InMemoryIdentityRepository::default();
        (entry_app_over(identities.clone()).0, identities)
    }

    fn entry_app_over(identities: InMemoryIdentityRepository) -> (Router, InMemoryEmailSender) {
        entry_app_over_v1(identities, V1Config::default())
    }

    /// Like `entry_app_over`, with the limits and the clock of the public API a test chooses.
    fn entry_app_over_v1(
        identities: InMemoryIdentityRepository,
        v1: V1Config,
    ) -> (Router, InMemoryEmailSender) {
        // Each app starts with nothing closed, even on a thread an earlier test used (`--test-threads=1`).
        LOCKS.with(|locks| locks.replace(InMemoryPeriodLockRepository::default()));
        let mailbox = InMemoryEmailSender::default();
        let purchase_store = InMemoryPurchaseOrderRepository::default();
        let categories = InMemoryCategoryRepository::default();
        let cost_centers = InMemoryCostCenterRepository::default();
        let parties = InMemoryPartyRepository::default();
        let bank_accounts = InMemoryBankAccountRepository::default();
        let payment_methods = InMemoryPaymentMethodRepository::default();
        let products = InMemoryProductRepository::default();
        let payment_terms = InMemoryPaymentTermRepository::default();
        let entry_store = InMemoryFinancialEntryRepository::default();
        // Sales, charges and stock tell the webhooks of this app what happens to them.
        let claims = InMemorySaleClaimRepository::default();
        let hooks = standalone_hooks();
        HOOKS.with(|held| held.replace(Some(hooks.clone())));
        let events: conta_verde_application::SharedEvents = Arc::new(V1Events::new(
            hooks.clone(),
            claims.clone(),
            entry_store.clone(),
            products.clone(),
            v1.clock.clone(),
        ));
        let charge_store = InMemoryChargeRepository::default().with_events(events.clone());
        let settings_store = InMemoryCollectionSettingsRepository::default();
        let transfer_store = InMemoryTransferRepository::default();
        let quote_store = InMemoryQuoteRepository::default();
        let sale_store = InMemorySaleRepository::default().with_events(events.clone());
        let ledger = InMemoryStockMovementRepository::default().with_events(events.clone());
        let stock_settings = InMemoryStockSettingsRepository::default();
        let attachment_store = InMemoryAttachmentRepository::default();
        let blob_store = InMemoryBlobStore::default();
        let fiscal_settings_store = InMemoryFiscalSettingsRepository::default();
        let sales = || {
            sales_over(
                sale_store.clone(),
                quote_store.clone(),
                entry_store.clone(),
                &categories,
                &cost_centers,
                &parties,
                &bank_accounts,
                &payment_methods,
                &products,
                &payment_terms,
                ledger.clone(),
                stock_settings.clone(),
            )
        };
        // The statements the web app lists are the ones the bank sync stores.
        let statement_store = InMemoryStatementLineRepository::default();
        let aggregator = FakeBankAggregator::default();
        AGGREGATOR.with(|held| held.replace(aggregator.clone()));
        let reconciliation = || {
            reconciliation_with(
                statement_store.clone(),
                entry_store.clone(),
                transfer_store.clone(),
                &categories,
                &cost_centers,
                &parties,
                &bank_accounts,
                &payment_methods,
            )
        };
        let app = router::<MemoryPorts>(
            Services {
                bank_sync: Arc::new(BankSyncService::new(
                    InMemoryBankConnectionRepository::default(),
                    aggregator,
                    reconciliation(),
                    InMemoryJobQueue::default(),
                )),
                webhook_subscriptions: hooks.clone(),
                api_keys: standalone_api_keys(),
                connectors: InMemoryConnectorRepository::default(),
                sale_claims: claims.clone(),
                parties: PartyService::new(parties.clone(), known_companies())
                    .with_recheck_interval(Duration::ZERO),
                products: ProductService::new(products.clone()),
                cost_centers: CostCenterService::new(cost_centers.clone()),
                categories: CategoryService::new(categories.clone()),
                bank_accounts: BankAccountService::new(bank_accounts.clone()),
                payment_methods: PaymentMethodService::new(payment_methods.clone()),
                payment_terms: PaymentTermService::new(payment_terms.clone()),
                entries: entries_over(
                    entry_store.clone(),
                    &categories,
                    &cost_centers,
                    &parties,
                    &bank_accounts,
                    &payment_methods,
                ),
                attachments: AttachmentService::new(
                    attachment_store.clone(),
                    blob_store.clone(),
                    entry_store.clone(),
                ),
                charges: ChargeService::new(
                    charge_store.clone(),
                    entry_store.clone(),
                    parties.clone(),
                    FakePaymentProvider::default(),
                    settings_store.clone(),
                ),
                webhooks: webhooks_over(
                    charge_store.clone(),
                    settings_store.clone(),
                    InMemoryPaymentEventRepository::default(),
                    entries_over(
                        entry_store.clone(),
                        &categories,
                        &cost_centers,
                        &parties,
                        &bank_accounts,
                        &payment_methods,
                    ),
                ),
                payment_pages: payment_pages_over(charge_store.clone(), entry_store.clone()),
                dunning: dunning_over(entry_store.clone(), charge_store.clone(), parties.clone()),
                collection: CollectionService::new(
                    settings_store.clone(),
                    references_over(
                        &categories,
                        &cost_centers,
                        &parties,
                        &bank_accounts,
                        &payment_methods,
                    ),
                ),
                transfers: TransferService::new(
                    transfer_store.clone(),
                    references_over(
                        &categories,
                        &cost_centers,
                        &parties,
                        &bank_accounts,
                        &payment_methods,
                    ),
                ),
                reconciliation: reconciliation(),
                recurrences: recurrences_over(
                    entry_store.clone(),
                    &categories,
                    &cost_centers,
                    &parties,
                    &bank_accounts,
                    &payment_methods,
                ),
                quotes: quotes_over(quote_store.clone(), &parties, &products, &payment_terms),
                sales: sales(),
                purchases: purchases_over(
                    purchase_store.clone(),
                    entry_store.clone(),
                    &categories,
                    &cost_centers,
                    &parties,
                    &bank_accounts,
                    &payment_methods,
                    &products,
                    &payment_terms,
                    ledger.clone(),
                    stock_settings.clone(),
                ),
                stock: StockService::new(ledger.clone(), products.clone(), stock_settings.clone())
                    .with_period_lock(locks()),
                counts: counts_over(
                    StockService::new(ledger.clone(), products.clone(), stock_settings.clone())
                        .with_period_lock(locks()),
                ),
                contracts: ContractService::new(InMemoryContractRepository::default(), sales()),
                commissions: commissions_over(
                    sale_store.clone(),
                    &parties,
                    &products,
                    &payment_terms,
                ),
                price_lists: price_lists_over(&parties, &products, &payment_terms),
                sales_reports: SalesReportService::new(sale_store.clone()),
                balances: BalanceService::new(
                    bank_accounts.clone(),
                    entry_store.clone(),
                    transfer_store.clone(),
                    InMemoryOpeningRepository::default(),
                )
                .with_period_lock(locks()),
                period_locks: PeriodLockService::new(locks()),
                packages: PackageScheduleService::new(
                    InMemoryPackageScheduleRepository::default(),
                    identities.clone(),
                    mailbox.clone(),
                    "http://localhost:5173",
                ),
                fiscal_documents: {
                    let gateway = FakeFiscalGateway::authorizing_after(1);
                    GATEWAY.with(|held| held.replace(gateway.clone()));
                    fiscal_documents_over(
                        &sale_store,
                        &parties,
                        &products,
                        &fiscal_settings_store,
                        &gateway,
                    )
                },
                fiscal_settings: FiscalSettingsService::new(fiscal_settings_store.clone()),
                gateway_settings: test_gateway_settings(),
                exports: AccountingExportService::new(
                    entry_store.clone(),
                    transfer_store.clone(),
                    attachment_store.clone(),
                    blob_store.clone(),
                    references_over(
                        &categories,
                        &cost_centers,
                        &parties,
                        &bank_accounts,
                        &payment_methods,
                    ),
                ),
                dre: DreService::new(entry_store.clone(), categories.clone()),
                postal_codes: PostalCodeService::new(known_postal_codes()),
                members: standalone_members(),
                documents: DocumentMailService::new(
                    purchase_store,
                    entry_store.clone(),
                    parties.clone(),
                    mailbox.clone(),
                    InMemoryDocumentMailRepository::default(),
                ),
                auth: AuthService::new(
                    identities,
                    Argon2SecretService::new().unwrap(),
                    Duration::from_secs(3600),
                ),
            },
            ApiConfig {
                fiscal_webhook: FiscalWebhook {
                    header: "x-fiscal-webhook-token".to_owned(),
                    secret: Some(FISCAL_HOOK_SECRET.to_owned()),
                },
                v1,
                ..ApiConfig::default()
            },
        );
        (app, mailbox)
    }

    /// The ids an entry points at, made through the API.
    struct EntryTargets {
        category: String,
        other_category: String,
        transfers: String,
        cost_center: String,
        party: String,
    }

    async fn entry_targets(app: &Router, cookie: &str, company_id: Uuid) -> EntryTargets {
        let id_of = |reply: Reply| {
            assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
            reply.body["id"].as_str().unwrap().to_owned()
        };
        let category = |name: &str, dre: &str, flow: &str| {
            post_category(
                app,
                cookie,
                company_id,
                json!({ "name": name, "dre_group": dre, "cash_flow_group": flow }),
            )
        };
        EntryTargets {
            category: id_of(category("Vendas", "gross_revenue", "operating").await),
            other_category: id_of(category("Despesas", "operating_expenses", "operating").await),
            transfers: id_of(category("Transferências", "transfers", "transfer").await),
            cost_center: id_of(
                post_cost_center(app, cookie, company_id, json!({ "name": "Matriz" })).await,
            ),
            party: id_of(
                send(
                    app,
                    json_request("POST", &parties_uri(company_id), Some(cookie), party_body()),
                )
                .await,
            ),
        }
    }

    fn entry_body(targets: &EntryTargets, description: &str, parts: &[(&str, &str)]) -> Value {
        json!({
            "kind": "receivable",
            "description": description,
            "party_id": targets.party,
            "category_id": targets.category,
            "cost_center_id": targets.cost_center,
            "competence_date": "2026-11-05",
            "installments": parts
                .iter()
                .map(|(due, amount)| json!({ "due_date": due, "amount": amount }))
                .collect::<Vec<_>>(),
        })
    }

    #[tokio::test]
    async fn the_accountant_downloads_the_books_of_a_month_exactly() {
        use conta_verde_domain::Role;

        let (app, identities) = entry_app_with_identities();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let accountant = member_with_role(&app, &identities, company_id, Role::Accountant).await;
        let viewer = member_with_role(&app, &identities, company_id, Role::Viewer).await;
        let (stranger, other_company) = sign_up(&app, "outra@example.com").await;
        let targets = entry_targets(&app, &owner, company_id).await;
        let others = entry_targets(&app, &stranger, other_company).await;
        let account = registered_account(&app, &owner, company_id, "Caixa").await;
        let other_account = registered_account(&app, &owner, company_id, "Banco").await;

        let enter = |who: &str,
                     with: &EntryTargets,
                     company: Uuid,
                     what: &str,
                     kind: &str,
                     category: &str,
                     competence: &str,
                     parts: &[(&str, &str)]| {
            let mut body = entry_body(with, what, parts);
            body["kind"] = json!(kind);
            body["category_id"] = json!(category);
            body["competence_date"] = json!(competence);
            let (app, who) = (app.clone(), who.to_owned());
            async move {
                let reply = post_to(&app, &entries_uri(company), &who, body).await;
                assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
                reply.body["id"].as_str().unwrap().to_owned()
            }
        };
        // October: a sale in two installments, with a description a spreadsheet would run, and a rent.
        let sale = enter(
            &owner,
            &targets,
            company_id,
            "Venda; \"da casa\"",
            "receivable",
            &targets.category,
            "2026-10-05",
            &[("2026-10-20", "600"), ("2026-11-20", "400")],
        )
        .await;
        let rent = enter(
            &owner,
            &targets,
            company_id,
            "=Aluguel",
            "payable",
            &targets.other_category,
            "2026-10-31",
            &[("2026-10-31", "200.5")],
        )
        .await;
        // Outside the span, and another company's: none of them is in the files.
        let september = enter(
            &owner,
            &targets,
            company_id,
            "Setembro",
            "receivable",
            &targets.category,
            "2026-09-30",
            &[("2026-10-10", "10")],
        )
        .await;
        enter(
            &owner,
            &targets,
            company_id,
            "Novembro",
            "receivable",
            &targets.category,
            "2026-11-01",
            &[("2026-11-30", "70")],
        )
        .await;
        enter(
            &stranger,
            &others,
            other_company,
            "Alheio",
            "receivable",
            &others.category,
            "2026-10-05",
            &[("2026-10-10", "77")],
        )
        .await;

        let settle = |entry: &str,
                      installment: u32,
                      on: &str,
                      applied: &str,
                      interest: Option<&str>| {
            let mut body = json!({ "installment": installment, "settled_on": on, "applied": applied, "bank_account_id": account });
            if let Some(interest) = interest {
                body["interest"] = json!(interest);
            }
            let uri = format!("{}/{entry}/settlements", entries_uri(company_id));
            let (app, owner) = (app.clone(), owner.clone());
            async move {
                let reply = post_to(&app, &uri, &owner, body).await;
                assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
                reply.body["id"].as_str().unwrap().to_owned()
            }
        };
        let paid_sale = settle(&sale, 1, "2026-10-20", "600", Some("5")).await;
        let paid_rent = settle(&rent, 1, "2026-10-31", "200.5", None).await;
        let undone = settle(&sale, 2, "2026-10-25", "100", None).await;
        settle(&sale, 2, "2026-11-03", "300", None).await;
        let reversed = send(
            &app,
            json_request(
                "DELETE",
                &format!("{}/{sale}/settlements/{undone}", entries_uri(company_id)),
                Some(&owner),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(reversed.status, StatusCode::NO_CONTENT, "{}", reversed.body);

        let transfer = |on: &str, amount: &str| {
            let (app, owner) = (app.clone(), owner.clone());
            let body = json!({ "from_account_id": account, "to_account_id": other_account, "amount": amount, "occurred_on": on });
            let uri = transfers_uri(company_id);
            async move {
                let reply = post_to(&app, &uri, &owner, body).await;
                assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
                reply.body["id"].as_str().unwrap().to_owned()
            }
        };
        let moved = transfer("2026-10-10", "30.25").await;
        let undone_transfer = transfer("2026-10-12", "99").await;
        transfer("2026-11-02", "5").await;
        let gone = send(
            &app,
            json_request(
                "DELETE",
                &format!("{}/{undone_transfer}", transfers_uri(company_id)),
                Some(&owner),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(gone.status, StatusCode::NO_CONTENT, "{}", gone.body);

        // A file attached to the sale is in the package; one on the September entry is not.
        let attach = |entry: &str, name: &str, bytes: &[u8]| {
            let uri = format!("{}?name={name}", attachments_uri(company_id, entry));
            let (app, owner, bytes) = (app.clone(), owner.clone(), bytes.to_vec());
            async move {
                let reply = send(
                    &app,
                    file_request("POST", &uri, Some(&owner), "application/pdf", bytes),
                )
                .await;
                assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
                reply.body["id"].as_str().unwrap().to_owned()
            }
        };
        let note = attach(&sale, "Nota%20%C3%A7.pdf", b"%PDF-1.4 nota").await;
        let lease = attach(&rent, "Contrato%3A%20aluguel.pdf", b"%PDF-1.4 contrato").await;
        attach(&september, "Antigo.pdf", b"%PDF-1.4 velho").await;

        let span = "from=2026-10-01&to=2026-10-31";
        let base = format!("/companies/{company_id}/accounting/exports");
        let get = |file: &str, query: &str, cookie: &str| {
            let request = get_request(&format!("{base}/{file}?{query}"), Some(cookie));
            let app = app.clone();
            async move { raw(&app, request).await }
        };

        // The entries: one row per installment, by competence date, in the exact layout of ADR 0071.
        let (status, headers, bytes) = get("entries", span, &accountant).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "text/csv; charset=utf-8");
        assert_eq!(
            headers[header::CONTENT_DISPOSITION],
            "attachment; filename=\"entries-2026-10-01-a-2026-10-31.csv\""
        );
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");
        let entries_csv = format!(
            "\u{feff}entry_id;kind;description;party;party_document;category;cost_center;competence_date;installment;installments;due_date;amount\r\n\
             {sale};receivable;\"Venda; \"\"da casa\"\"\";Padaria Central;11222333000181;Vendas;Matriz;2026-10-05;1;2;2026-10-20;600\r\n\
             {sale};receivable;\"Venda; \"\"da casa\"\"\";Padaria Central;11222333000181;Vendas;Matriz;2026-10-05;2;2;2026-11-20;400\r\n\
             {rent};payable;'=Aluguel;Padaria Central;11222333000181;Despesas;Matriz;2026-10-31;1;1;2026-10-31;200.5\r\n"
        );
        assert_eq!(String::from_utf8(bytes).unwrap(), entries_csv);

        // The settlements: by the day settled, what was reversed out, the cash worked out.
        let (status, _, bytes) = get("settlements", span, &accountant).await;
        assert_eq!(status, StatusCode::OK);
        let settlements_csv = format!(
            "\u{feff}settlement_id;entry_id;kind;description;party;category;cost_center;installment;settled_on;bank_account;payment_method;applied;interest;fine;discount;cash\r\n\
             {paid_sale};{sale};receivable;\"Venda; \"\"da casa\"\"\";Padaria Central;Vendas;Matriz;1;2026-10-20;Caixa;;600;5;0;0;605\r\n\
             {paid_rent};{rent};payable;'=Aluguel;Padaria Central;Despesas;Matriz;1;2026-10-31;Caixa;;200.5;0;0;0;200.5\r\n"
        );
        assert_eq!(String::from_utf8(bytes).unwrap(), settlements_csv);

        // The transfers: the one in the span, not the reversed one.
        let (status, _, bytes) = get("transfers", span, &accountant).await;
        assert_eq!(status, StatusCode::OK);
        let transfers_csv = format!(
            "\u{feff}transfer_id;occurred_on;from_account;to_account;amount;note\r\n\
             {moved};2026-10-10;Caixa;Banco;30.25;\r\n"
        );
        assert_eq!(String::from_utf8(bytes).unwrap(), transfers_csv);

        // The package: the same three files, the list of what is attached and the file itself, and nothing else.
        let (status, headers, bytes) = get("package", span, &accountant).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "application/zip");
        assert_eq!(
            headers[header::CONTENT_DISPOSITION],
            "attachment; filename=\"package-2026-10-01-a-2026-10-31.zip\""
        );
        // In the order of the entries file (the sale, then the rent), whatever the order of their ids; a name that
        // Windows cannot unpack is made one it can.
        let note_path = format!("attachments/{sale}/{note}-Nota ç.pdf");
        let lease_path = format!("attachments/{rent}/{lease}-Contrato_ aluguel.pdf");
        let attachments_csv = format!(
            "\u{feff}attachment_id;entry_id;file_name;content_type;size;path\r\n\
             {note};{sale};Nota ç.pdf;application/pdf;13;{note_path}\r\n\
             {lease};{rent};Contrato: aluguel.pdf;application/pdf;17;{lease_path}\r\n"
        );
        let fiscal_csv = "\u{feff}document_id;kind;status;number;series;access_key;issued_on;recipient_name;recipient_document;total;environment;nature;xml_path;pdf_path;cancellation_path\r\n";
        let files = crate::export_files::tests::unzip(&bytes);
        assert_eq!(
            files,
            vec![
                ("entries.csv".to_owned(), entries_csv.clone().into_bytes()),
                ("settlements.csv".to_owned(), settlements_csv.into_bytes()),
                ("transfers.csv".to_owned(), transfers_csv.into_bytes()),
                ("attachments.csv".to_owned(), attachments_csv.into_bytes()),
                // No fiscal document was issued in production: the list holds its header only.
                ("fiscal.csv".to_owned(), fiscal_csv.as_bytes().to_vec()),
                (note_path, b"%PDF-1.4 nota".to_vec()),
                (lease_path, b"%PDF-1.4 contrato".to_vec()),
            ]
        );

        // The package as spreadsheets: the same files by name, the list of attachments still a CSV.
        let (status, _, bytes) = get("package", &format!("{span}&format=xlsx"), &accountant).await;
        assert_eq!(status, StatusCode::OK);
        let names: Vec<String> = crate::export_files::tests::unzip(&bytes)
            .into_iter()
            .map(|(name, _)| name)
            .take(4)
            .collect();
        assert_eq!(
            names,
            [
                "entries.xlsx",
                "settlements.xlsx",
                "transfers.xlsx",
                "attachments.csv"
            ]
        );

        // The same entries as a spreadsheet: a zip of its five parts, the amounts as numbers.
        let (status, headers, bytes) =
            get("entries", &format!("{span}&format=xlsx"), &accountant).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers[header::CONTENT_TYPE],
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
        assert_eq!(
            headers[header::CONTENT_DISPOSITION],
            "attachment; filename=\"entries-2026-10-01-a-2026-10-31.xlsx\""
        );
        let sheet = crate::export_files::tests::unzip(&bytes);
        let sheet = String::from_utf8(sheet.last().unwrap().1.clone()).unwrap();
        assert!(sheet.contains("Venda; &quot;da casa&quot;"), "{sheet}");
        assert!(sheet.contains("<c r=\"L4\"><v>200.5</v></c>"), "{sheet}");

        // Nothing of the outside or of another company's is in any of it.
        let everything = String::from_utf8_lossy(&bytes).to_string() + &entries_csv;
        for absent in ["Setembro", "Novembro", "Alheio", "Antigo"] {
            assert!(!everything.contains(absent), "{absent}");
        }

        // A reader of the area can download; one outside the company cannot see it; a bad request is told why.
        assert_eq!(get("entries", span, &viewer).await.0, StatusCode::OK);
        assert_eq!(get("package", span, &viewer).await.0, StatusCode::OK);
        assert_eq!(
            get("entries", span, &stranger).await.0,
            StatusCode::NOT_FOUND
        );
        let unknown = [
            (
                "entries",
                "from=2026-10-01&to=2026-10-31&format=pdf".to_owned(),
            ),
            ("entries", "from=2026-10-01".to_owned()),
            ("entries", "from=october&to=2026-10-31".to_owned()),
            ("settlements", "from=2026-10-31&to=2026-10-01".to_owned()),
            ("transfers", "from=2025-01-01&to=2026-12-31".to_owned()),
            ("package", "from=2025-01-01&to=2026-12-31".to_owned()),
        ];
        for (file, query) in unknown {
            let (status, _, body) = get(file, &query, &accountant).await;
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "{file} {query}: {}",
                String::from_utf8_lossy(&body)
            );
        }
        assert_eq!(
            send(&app, get_request(&format!("{base}/entries?{span}"), None))
                .await
                .status,
            StatusCode::UNAUTHORIZED
        );
    }

    fn fiscal_settings_body() -> Value {
        json!({
            "legal_name": "Verde Comércio Ltda",
            "cnpj": "11.222.333/0001-81",
            "state_registration": "110.042.490.114",
            "municipal_registration": null,
            "address": {
                "postal_code": "01310-100",
                "street": "Avenida Paulista",
                "number": "1000",
                "complement": null,
                "district": "Bela Vista",
                "city": "São Paulo",
                "state": "SP"
            },
            "regime": "simples_nacional",
            "environment": "homologation",
            "nfe_series": 1,
            "nfce_series": 1,
            "nfse_series": 1,
            "certificate_valid_until": "2027-03-31",
            "city_ibge_code": "3550308",
            "natures": [{
                "code": "service",
                "name": "Prestação de serviço",
                "kind": "service",
                "cfop_in_state": null,
                "cfop_interstate": null,
                "active": true,
                "rules": [],
                "service": {
                    "national_tax_code": "010701",
                    "municipal_service_code": "1.07",
                    "iss_taxation": "taxable",
                    "iss_rate": "2.5"
                }
            }, {
                "code": "sale",
                "name": "Venda de mercadoria",
                "kind": "product",
                "cfop_in_state": "5102",
                "cfop_interstate": "6102",
                "active": true,
                "rules": [{
                    "regime": "simples_nacional",
                    "ncm_prefix": null,
                    "icms_code": "102",
                    "pis_code": "07",
                    "cofins_code": "07",
                    "icms_rate": null,
                    "pis_rate": null,
                    "cofins_rate": null
                }],
                "service": null
            }]
        })
    }

    /// The fiscal settings ([ADR 0074]): `null` until set, written by whoever writes the fiscal area (the owner and
    /// the financial member), read by the others, refused for a shape the domain rejects, and every change in the trail.
    #[tokio::test]
    async fn the_fiscal_settings_are_set_by_those_who_write_the_fiscal_area() {
        use conta_verde_domain::Role;

        let (app, identities) = app_with_identities();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let financial = member_with_role(&app, &identities, company_id, Role::Financial).await;
        let seller = member_with_role(&app, &identities, company_id, Role::Sales).await;
        let accountant = member_with_role(&app, &identities, company_id, Role::Accountant).await;
        let (stranger, _) = sign_up(&app, "outra@example.com").await;
        let uri = format!("/companies/{company_id}/fiscal/settings");

        // Nothing set yet, for everyone who may look.
        for cookie in [&owner, &seller, &accountant] {
            let read = send(&app, get_request(&uri, Some(cookie))).await;
            assert_eq!(read.status, StatusCode::OK, "{}", read.body);
            assert_eq!(read.body, Value::Null);
        }

        // The ones who only read are refused; the financial member and the owner write.
        for cookie in [&seller, &accountant] {
            let denied = send(
                &app,
                json_request("PUT", &uri, Some(cookie), fiscal_settings_body()),
            )
            .await;
            assert_eq!(denied.status, StatusCode::FORBIDDEN);
        }
        let saved = send(
            &app,
            json_request("PUT", &uri, Some(&financial), fiscal_settings_body()),
        )
        .await;
        assert_eq!(saved.status, StatusCode::OK, "{}", saved.body);
        // Stored without punctuation, and read back as sent otherwise.
        assert_eq!(saved.body["cnpj"], "11222333000181");
        assert_eq!(saved.body["state_registration"], "110042490114");
        assert_eq!(saved.body["address"]["postal_code"], "01310100");
        assert_eq!(saved.body["natures"][1]["rules"][0]["icms_code"], "102");
        assert_eq!(saved.body["city_ibge_code"], "3550308");
        assert_eq!(
            saved.body["natures"][0]["service"],
            json!({
                "national_tax_code": "010701",
                "municipal_service_code": "1.07",
                "iss_taxation": "taxable",
                "iss_rate": "2.5"
            })
        );
        let read = send(&app, get_request(&uri, Some(&seller))).await;
        assert_eq!(read.body, saved.body);

        // A shape the domain refuses is a 422 that says which field; a value that is no key of ours too; an unknown
        // field is refused; and nothing of it was kept.
        let mut bad_cfop = fiscal_settings_body();
        bad_cfop["natures"][1]["cfop_in_state"] = json!("6102");
        let refused = send(&app, json_request("PUT", &uri, Some(&owner), bad_cfop)).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            refused.body["error"]
                .as_str()
                .unwrap()
                .contains("cfop_in_state"),
            "{}",
            refused.body
        );
        let mut bad_terms = fiscal_settings_body();
        bad_terms["natures"][0]["service"]["iss_taxation"] = json!("whatever");
        let refused = send(&app, json_request("PUT", &uri, Some(&owner), bad_terms)).await;
        assert_eq!(
            refused.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            refused.body
        );
        let mut bad_code = fiscal_settings_body();
        bad_code["natures"][0]["service"]["national_tax_code"] = json!("10701");
        let refused = send(&app, json_request("PUT", &uri, Some(&owner), bad_code)).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        let mut mei = fiscal_settings_body();
        mei["regime"] = json!("mei");
        let refused = send(&app, json_request("PUT", &uri, Some(&owner), mei)).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        let mut production = fiscal_settings_body();
        production["environment"] = json!("production");
        production["certificate_valid_until"] = Value::Null;
        let refused = send(&app, json_request("PUT", &uri, Some(&owner), production)).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        let mut extra = fiscal_settings_body();
        extra["certificate_password"] = json!("never");
        let refused = send(&app, json_request("PUT", &uri, Some(&owner), extra)).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            send(&app, get_request(&uri, Some(&owner))).await.body,
            saved.body
        );

        // The owner changes the regime and the series; the trail has the creation and the change.
        let mut changed = fiscal_settings_body();
        changed["nfe_series"] = json!(2);
        let updated = send(&app, json_request("PUT", &uri, Some(&owner), changed)).await;
        assert_eq!(updated.status, StatusCode::OK, "{}", updated.body);
        let history = send(&app, get_request(&format!("{uri}/history"), Some(&seller))).await;
        assert_eq!(history.status, StatusCode::OK);
        let trail = history.body.as_array().unwrap();
        assert_eq!(trail.len(), 2);
        assert_eq!(trail[1]["action"], "updated");
        assert_eq!(trail[1]["changes"][0]["field"], "nfe_series");

        // Another company's member sees none of it, and neither can write it.
        let theirs = format!("/companies/{company_id}/fiscal/settings");
        assert_eq!(
            send(&app, get_request(&theirs, Some(&stranger)))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&theirs, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    /// Which gateway the installation issues through ([ADR 0075]): anyone who reaches the company may ask, and the
    /// answer is the adapter's name (or `null` for none, which the in-memory test ports never are).
    #[tokio::test]
    async fn the_fiscal_gateway_is_told_to_those_who_read_the_fiscal_area() {
        use conta_verde_domain::Role;

        let (app, identities) = app_with_identities();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let viewer = member_with_role(&app, &identities, company_id, Role::Viewer).await;
        let uri = format!("/companies/{company_id}/fiscal/gateway");
        let reply = send(&app, get_request(&uri, Some(&viewer))).await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
        assert_eq!(reply.body, json!({ "gateway": "fake" }));
        assert_eq!(
            send(&app, get_request(&uri, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn the_accountant_chooses_when_the_package_is_announced() {
        use conta_verde_domain::Role;

        let (app, identities) = entry_app_with_identities();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let accountant = member_with_role(&app, &identities, company_id, Role::Accountant).await;
        let viewer = member_with_role(&app, &identities, company_id, Role::Viewer).await;
        let (stranger, _) = sign_up(&app, "outra@example.com").await;
        let uri = format!("/companies/{company_id}/accounting/package-schedule");

        // Off on the 5th until chosen, and it says whom it would reach: the one accountant, not the viewer.
        let read = send(&app, get_request(&uri, Some(&viewer))).await;
        assert_eq!(read.status, StatusCode::OK, "{}", read.body);
        assert_eq!(
            read.body,
            json!({ "enabled": false, "day": 5, "accountants": 1 })
        );

        // A reader cannot choose; the accountant can, and so can the owner.
        let denied = send(
            &app,
            json_request(
                "PUT",
                &uri,
                Some(&viewer),
                json!({ "enabled": true, "day": 10 }),
            ),
        )
        .await;
        assert_eq!(denied.status, StatusCode::FORBIDDEN);
        let chosen = send(
            &app,
            json_request(
                "PUT",
                &uri,
                Some(&accountant),
                json!({ "enabled": true, "day": 10 }),
            ),
        )
        .await;
        assert_eq!(chosen.status, StatusCode::OK, "{}", chosen.body);
        assert_eq!(
            chosen.body,
            json!({ "enabled": true, "day": 10, "accountants": 1 })
        );
        let owner_choice = send(
            &app,
            json_request(
                "PUT",
                &uri,
                Some(&owner),
                json!({ "enabled": true, "day": 12 }),
            ),
        )
        .await;
        assert_eq!(owner_choice.status, StatusCode::OK);
        assert_eq!(
            send(&app, get_request(&uri, Some(&viewer))).await.body["day"],
            12
        );

        // A day every month has, and both fields said every time.
        for body in [
            json!({ "enabled": true, "day": 0 }),
            json!({ "enabled": true, "day": 29 }),
            json!({ "enabled": true }),
            json!({ "day": 10 }),
            json!({ "enabled": true, "day": 10, "to": "x@example.com" }),
        ] {
            let refused = send(
                &app,
                json_request("PUT", &uri, Some(&accountant), body.clone()),
            )
            .await;
            assert_eq!(
                refused.status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{body}: {}",
                refused.body
            );
        }
        assert_eq!(
            send(&app, get_request(&uri, Some(&viewer))).await.body["day"],
            12,
            "what was refused changed nothing"
        );

        // Who chose what is in the trail, and nobody outside the company sees any of it.
        let history = send(&app, get_request(&format!("{uri}/history"), Some(&viewer))).await;
        assert_eq!(history.status, StatusCode::OK);
        assert_eq!(
            history.body.as_array().unwrap().len(),
            2,
            "two choices, none of the refused ones"
        );
        assert_eq!(history.body[0]["action"], "created");
        assert_eq!(history.body[1]["changes"][0]["field"], "day");
        assert_eq!(
            send(&app, get_request(&uri, Some(&stranger))).await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&uri, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn the_income_statement_adds_the_entries_up_by_month_and_cost_center() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let enter = |kind: &str,
                     category: &str,
                     competence: &str,
                     cost_center: Option<&str>,
                     amount: &str| {
            let mut body = entry_body(&targets, "Lançamento", &[("2027-03-10", amount)]);
            body["kind"] = json!(kind);
            body["category_id"] = json!(category);
            body["competence_date"] = json!(competence);
            body["cost_center_id"] = json!(cost_center);
            body
        };
        for body in [
            enter(
                "receivable",
                &targets.category,
                "2026-10-05",
                Some(&targets.cost_center),
                "100",
            ),
            enter("receivable", &targets.category, "2026-10-20", None, "50.5"),
            enter(
                "payable",
                &targets.other_category,
                "2026-11-03",
                Some(&targets.cost_center),
                "40",
            ),
        ] {
            let made = post_to(&app, &entries_uri(company_id), &cookie, body).await;
            assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
        }
        let uri = |query: &str| format!("/companies/{company_id}/dre?{query}");

        let by_month = send(
            &app,
            get_request(&uri("from=2026-10-01&to=2026-11-30"), Some(&cookie)),
        )
        .await;
        assert_eq!(by_month.status, StatusCode::OK, "{}", by_month.body);
        assert_eq!(by_month.body["by"], "month");
        let columns = by_month.body["columns"].as_array().unwrap();
        assert_eq!(
            columns
                .iter()
                .map(|column| column["key"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["2026-10", "2026-11"]
        );
        assert_eq!(columns[0]["lines"]["gross_revenue"], "150.5");
        assert_eq!(columns[0]["lines"]["net_result"], "150.5");
        assert_eq!(columns[1]["lines"]["operating_expenses"], "-40");
        assert_eq!(columns[1]["lines"]["net_result"], "-40");
        assert_eq!(by_month.body["total"]["gross_profit"], "150.5");
        assert_eq!(by_month.body["total"]["operating_result"], "110.5");
        assert_eq!(by_month.body["total"]["net_result"], "110.5");

        let by_center = send(
            &app,
            get_request(
                &uri("from=2026-10-01&to=2026-11-30&by=cost_center"),
                Some(&cookie),
            ),
        )
        .await;
        let columns = by_center.body["columns"].as_array().unwrap();
        assert_eq!(columns[0]["key"], targets.cost_center);
        assert_eq!(columns[0]["lines"]["net_result"], "60");
        assert_eq!(columns[1]["key"], Value::Null);
        assert_eq!(columns[1]["lines"]["net_result"], "50.5");

        let without = send(
            &app,
            get_request(
                &uri("from=2026-10-01&to=2026-11-30&cost_center=none"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(without.body["total"]["net_result"], "50.5");
        let only = send(
            &app,
            get_request(
                &uri(&format!(
                    "from=2026-10-01&to=2026-11-30&cost_center={}",
                    targets.cost_center
                )),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(only.body["total"]["net_result"], "60");

        for (query, status) in [
            ("to=2026-11-30", StatusCode::BAD_REQUEST),
            (
                "from=2026-10-01&to=2026-11-30&by=year",
                StatusCode::BAD_REQUEST,
            ),
            (
                "from=2026-10-01&to=2026-11-30&cost_center=matriz",
                StatusCode::BAD_REQUEST,
            ),
            ("from=2026-12-01&to=2026-11-30", StatusCode::BAD_REQUEST),
            ("from=2024-01-01&to=2026-11-30", StatusCode::BAD_REQUEST),
        ] {
            let reply = send(&app, get_request(&uri(query), Some(&cookie))).await;
            assert_eq!(reply.status, status, "{query}: {}", reply.body);
            assert!(reply.body["error"].is_string(), "{query}");
        }
        assert_eq!(
            send(
                &app,
                get_request(&uri("from=2026-10-01&to=2026-10-31"), None)
            )
            .await
            .status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn imports_entries_from_a_csv_file_once_and_names_every_refused_row() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let child = post_category(
            &app,
            &cookie,
            company_id,
            json!({ "name": "Mensalidades", "parent_id": targets.category }),
        )
        .await;
        assert_eq!(child.status, StatusCode::CREATED, "{}", child.body);
        let account = post_to(
            &app,
            &bank_accounts_uri(company_id),
            &cookie,
            json!({ "kind": "checking", "name": "Conta principal" }),
        )
        .await;
        assert_eq!(account.status, StatusCode::CREATED, "{}", account.body);
        let uri = format!("/companies/{company_id}/entries/import");
        let header = "reference;kind;description;party_name;category;cost_center;competence_date;due_date;amount;settled_on;interest;bank_account\n";
        let file = format!(
            "{header}\
             ca-1;receivable;Mensalidade de março;Padaria Central;Vendas > Mensalidades;Matriz;2026-03-10;2026-03-15;100.50;2026-03-16;1.5;Conta principal\n\
             ca-2;payable;Material;;Despesas;;2026-03-11;2026-03-20;40;;;\n"
        );

        let dry = send(
            &app,
            csv_request(&format!("{uri}?dry_run=true"), Some(&cookie), file.clone()),
        )
        .await;
        assert_eq!(dry.status, StatusCode::OK, "{}", dry.body);
        assert_eq!(
            (dry.body["valid"].clone(), dry.body["created"].clone()),
            (json!(2), json!(0))
        );
        let listed = send(&app, get_request(&entries_uri(company_id), Some(&cookie))).await;
        assert_eq!(listed.body["items"], json!([]));

        let done = send(&app, csv_request(&uri, Some(&cookie), file.clone())).await;
        assert_eq!(done.status, StatusCode::OK, "{}", done.body);
        assert_eq!(done.body["created"], 2);
        let listed = send(&app, get_request(&entries_uri(company_id), Some(&cookie))).await;
        let items = listed.body["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        let paid = items
            .iter()
            .find(|item| item["description"] == "Mensalidade de março")
            .unwrap();
        assert_eq!(paid["party_id"], targets.party);
        assert_eq!(paid["installments"][0]["settled"], "100.5");

        let again = send(&app, csv_request(&uri, Some(&cookie), file)).await;
        assert_eq!(
            (
                again.body["created"].clone(),
                again.body["skipped"].as_array().unwrap().len()
            ),
            (json!(0), 2)
        );

        // A refused row refuses the file, and the reply names the row.
        let refused = send(
            &app,
            csv_request(
                &uri,
                Some(&cookie),
                format!("{header}ca-3;payable;Outro;;Inexistente;;2026-03-11;2026-03-20;10;;;\n"),
            ),
        )
        .await;
        assert_eq!(refused.status, StatusCode::OK);
        assert_eq!(refused.body["created"], 0);
        assert_eq!(refused.body["refused"][0]["row"], 2);
        assert!(
            refused.body["refused"][0]["message"]
                .as_str()
                .unwrap()
                .contains("Inexistente")
        );

        let unknown = send(
            &app,
            csv_request(
                &uri,
                Some(&cookie),
                "reference;kind;surprise\nx;payable;1\n",
            ),
        )
        .await;
        assert_eq!(unknown.status, StatusCode::UNPROCESSABLE_ENTITY);
        let anonymous = send(&app, csv_request(&uri, None, "reference\n1\n")).await;
        assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn registers_changes_and_lists_entries_with_their_history() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let uri = entries_uri(company_id);

        let created = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                entry_body(
                    &targets,
                    "Venda parcelada",
                    &[("2026-12-10", "100.50"), ("2027-01-10", "100.5")],
                ),
            ),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        // Amounts are decimal strings without trailing zeros, the total is added up, and the
        // installments come numbered.
        assert_eq!(created.body["total"], "201");
        assert_eq!(created.body["kind"], "receivable");
        assert_eq!(
            created.body["installments"][0],
            json!({ "number": 1, "due_date": "2026-12-10", "amount": "100.5", "settled": "0" })
        );
        assert_eq!(created.body["installments"][1]["number"], 2);
        assert_eq!(created.body["party_id"], targets.party);
        let id = created.body["id"].as_str().unwrap().to_owned();
        let one = format!("{uri}/{id}");

        let fetched = send(&app, get_request(&one, Some(&cookie))).await;
        assert_eq!(fetched.status, StatusCode::OK);
        assert_eq!(fetched.body, created.body);

        // Replaced as a whole: the references left out are removed, the installments are the new ones.
        let replaced = send(
            &app,
            json_request(
                "PUT",
                &one,
                Some(&cookie),
                json!({
                    "kind": "payable",
                    "description": "Aluguel",
                    "category_id": targets.other_category,
                    "competence_date": "2026-11-01",
                    "installments": [{ "due_date": "2026-11-10", "amount": "2500" }],
                }),
            ),
        )
        .await;
        assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.body);
        assert_eq!(replaced.body["party_id"], Value::Null);
        assert_eq!(replaced.body["cost_center_id"], Value::Null);
        assert_eq!(replaced.body["total"], "2500");

        // The same body again changes nothing and records nothing.
        let same = replaced.body.clone();
        let again = send(
            &app,
            json_request(
                "PUT",
                &one,
                Some(&cookie),
                json!({
                    "kind": same["kind"], "description": same["description"],
                    "category_id": same["category_id"], "competence_date": same["competence_date"],
                    "installments": [{ "due_date": "2026-11-10", "amount": "2500" }],
                }),
            ),
        )
        .await;
        assert_eq!(again.status, StatusCode::OK);
        let history = send(&app, get_request(&format!("{one}/history"), Some(&cookie))).await;
        assert_eq!(history.status, StatusCode::OK);
        let trail = history.body.as_array().unwrap();
        assert_eq!(trail.len(), 2);
        assert_eq!(trail[0]["action"], "created");
        assert_eq!(trail[1]["action"], "updated");
        let changed: Vec<&str> = trail[1]["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|change| change["field"].as_str().unwrap())
            .collect();
        assert!(
            changed.contains(&"installments") && changed.contains(&"party_id"),
            "{changed:?}"
        );

        let listed = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(listed.body["next_cursor"], Value::Null);
    }

    #[tokio::test]
    async fn an_entry_that_points_at_what_cannot_be_used_or_says_what_is_not_a_date_is_refused_in_json()
     {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let uri = entries_uri(company_id);
        let post = |body: Value| send(&app, json_request("POST", &uri, Some(&cookie), body));
        let ok = entry_body(&targets, "x", &[("2026-12-10", "10")]);
        let with = |field: &str, value: Value| {
            let mut body = ok.clone();
            body[field] = value;
            body
        };

        // What cannot be pointed at is a 422 naming the field: the transfers category, a record
        // that does not exist, and one that belongs to another company.
        let transfers = post(with("category_id", json!(targets.transfers))).await;
        assert_eq!(transfers.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            transfers.body["error"]
                .as_str()
                .unwrap()
                .contains("category")
        );
        for (field, value) in [
            ("category_id", json!(Uuid::new_v4())),
            ("cost_center_id", json!(Uuid::new_v4())),
            ("party_id", json!(Uuid::new_v4())),
        ] {
            let reply = post(with(field, value)).await;
            assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{field}");
        }
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        let stolen = send(
            &app,
            json_request("POST", &uri, Some(&stranger), ok.clone()),
        )
        .await;
        assert_eq!(
            stolen.status,
            StatusCode::NOT_FOUND,
            "not a member of this company"
        );

        // The domain's rules are 422s that say what to change; a malformed body is JSON too.
        for (body, what) in [
            (
                with("competence_date", json!("05/11/2026")),
                "competence_date",
            ),
            (with("description", json!("")), "description"),
            (with("installments", json!([])), "at least one installment"),
            (
                with(
                    "installments",
                    json!([{ "due_date": "2026-12-10", "amount": "1.005" }]),
                ),
                "cents",
            ),
            (
                with(
                    "installments",
                    json!([{ "due_date": "2026-12-10", "amount": "0" }]),
                ),
                "cents",
            ),
            (
                with(
                    "installments",
                    json!([{ "due_date": "2026-12-10", "amount": "-1" }]),
                ),
                "cents",
            ),
            (
                with(
                    "installments",
                    json!([{ "due_date": "2026-12-11", "amount": "1" }, { "due_date": "2026-12-10", "amount": "1" }]),
                ),
                "decrease",
            ),
            (
                with(
                    "installments",
                    json!([{ "due_date": "2026-13-01", "amount": "1" }]),
                ),
                "due_date",
            ),
            (with("kind", json!("sale")), "receivable or payable"),
        ] {
            let reply = post(body.clone()).await;
            assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert!(
                reply.body["error"].as_str().unwrap().contains(what),
                "{}",
                reply.body
            );
        }
        // Amounts are strings, never JSON numbers; unknown fields are refused, not ignored.
        let number = post(with(
            "installments",
            json!([{ "due_date": "2026-12-10", "amount": 10 }]),
        ))
        .await;
        assert_eq!(number.status, StatusCode::UNPROCESSABLE_ENTITY);
        let unknown = post(with("total", json!("10"))).await;
        assert_eq!(unknown.status, StatusCode::UNPROCESSABLE_ENTITY);
        let none = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(
            none.body["items"].as_array().unwrap().len(),
            0,
            "a refused entry is not stored"
        );

        // An id that is not one is a JSON 404 like any other miss.
        let bad_id = send(
            &app,
            get_request(&format!("{uri}/not-an-id"), Some(&cookie)),
        )
        .await;
        assert_eq!(bad_id.status, StatusCode::NOT_FOUND);
        assert!(bad_id.body["error"].is_string());
    }

    #[tokio::test]
    async fn lists_entries_newest_first_by_page_and_filters_them_from_query_parameters() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let uri = entries_uri(company_id);
        for (description, kind, category, competence) in [
            ("Aluguel", "payable", &targets.other_category, "2026-09-01"),
            (
                "Venda de bolos",
                "receivable",
                &targets.category,
                "2026-10-01",
            ),
            (
                "Fornecedor",
                "payable",
                &targets.other_category,
                "2026-11-01",
            ),
        ] {
            let mut body = entry_body(&targets, description, &[("2026-12-10", "10")]);
            body["kind"] = json!(kind);
            body["category_id"] = json!(category);
            body["competence_date"] = json!(competence);
            let reply = send(&app, json_request("POST", &uri, Some(&cookie), body)).await;
            assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
        }
        let descriptions = |reply: &Reply| -> Vec<String> {
            reply.body["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["description"].as_str().unwrap().to_owned())
                .collect()
        };
        let list = |query: &str| send(&app, get_request(&format!("{uri}?{query}"), Some(&cookie)));

        assert_eq!(
            descriptions(&list("").await),
            ["Fornecedor", "Venda de bolos", "Aluguel"]
        );
        assert_eq!(
            descriptions(&list("sort=competence_date").await),
            ["Aluguel", "Venda de bolos", "Fornecedor"]
        );
        assert_eq!(
            descriptions(&list("kind=payable").await),
            ["Fornecedor", "Aluguel"]
        );
        assert_eq!(
            descriptions(&list(&format!("category={}", targets.category)).await),
            ["Venda de bolos"]
        );
        assert_eq!(descriptions(&list("q=VENDA").await), ["Venda de bolos"]);

        // A page says where the next one starts, and the cursor resumes right after it.
        let first = list("limit=2").await;
        assert_eq!(descriptions(&first), ["Fornecedor", "Venda de bolos"]);
        let cursor = first.body["next_cursor"].as_str().unwrap().to_owned();
        let second = list(&format!("limit=2&after={cursor}")).await;
        assert_eq!(descriptions(&second), ["Aluguel"]);
        assert_eq!(second.body["next_cursor"], Value::Null);

        // What is not understood is a 400 in JSON, never ignored.
        for query in [
            "kind=sale",
            "sort=name",
            "category=abc",
            "after=garbage",
            "after=x|2026-01-01",
        ] {
            let reply = list(query).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{query}");
            assert!(reply.body["error"].is_string(), "{query}");
        }
    }

    #[tokio::test]
    async fn entries_stay_private_and_follow_the_finance_role() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let uri = entries_uri(company_id);
        // A member of another company does not even learn that the company exists.
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        assert_eq!(
            send(&app, get_request(&uri, Some(&stranger))).await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&uri, None)).await.status,
            StatusCode::UNAUTHORIZED
        );

        // Everyone who belongs reads; only the roles that write finance register, and the others are
        // refused with 403 before the body is looked at.
        for (role, may_write) in [
            (Role::Admin, true),
            (Role::Financial, true),
            (Role::Sales, false),
            (Role::Stock, false),
            (Role::Accountant, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&uri, Some(&cookie))).await.status,
                StatusCode::OK,
                "{role:?} reads"
            );
            let write = send(&app, json_request("POST", &uri, Some(&cookie), json!({}))).await;
            let expected = if may_write {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(write.status, expected, "{role:?} writes");
        }
    }

    async fn registered_account(
        app: &Router,
        cookie: &str,
        company_id: Uuid,
        name: &str,
    ) -> String {
        let reply = send(
            app,
            json_request(
                "POST",
                &format!("/companies/{company_id}/bank-accounts"),
                Some(cookie),
                json!({ "kind": "cash", "name": name }),
            ),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
        reply.body["id"].as_str().unwrap().to_owned()
    }

    #[tokio::test]
    async fn settles_an_installment_in_part_and_in_full_and_reverses_it_through_the_api() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let account = registered_account(&app, &cookie, company_id, "Caixa").await;
        let uri = entries_uri(company_id);
        let created = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                entry_body(
                    &targets,
                    "Venda",
                    &[("2026-12-10", "100"), ("2027-01-10", "50.5")],
                ),
            ),
        )
        .await;
        let entry = format!("{uri}/{}", created.body["id"].as_str().unwrap());
        let settlements = format!("{entry}/settlements");
        let settle = |body: Value| {
            send(
                &app,
                json_request("POST", &settlements, Some(&cookie), body),
            )
        };
        let body = |installment: u32, applied: &str| json!({ "installment": installment, "settled_on": "2026-12-10", "applied": applied, "bank_account_id": account });

        // Nothing is settled yet, and what has been settled is said per installment.
        assert_eq!(created.body["installments"][0]["settled"], "0");

        let part = settle(json!({
            "installment": 1, "settled_on": "2026-12-09", "applied": "40",
            "interest": "1.5", "fine": "0.5", "discount": "2", "bank_account_id": account,
        }))
        .await;
        assert_eq!(part.status, StatusCode::CREATED, "{}", part.body);
        assert_eq!(part.body["cash"], "40");
        assert_eq!(part.body["applied"], "40");
        assert_eq!(part.body["reversed"], false);
        assert_eq!(part.body["bank_account_id"], account);
        let rest = settle(body(1, "60")).await;
        assert_eq!(rest.status, StatusCode::CREATED);

        let fetched = send(&app, get_request(&entry, Some(&cookie))).await;
        assert_eq!(fetched.body["installments"][0]["settled"], "100");
        assert_eq!(fetched.body["installments"][1]["settled"], "0");
        let listed = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(listed.body["items"][0]["installments"][0]["settled"], "100");

        // Not for more than is left; a refusal is a 409 that says why, and nothing is stored.
        let over = settle(body(1, "0.01")).await;
        assert_eq!(over.status, StatusCode::CONFLICT);
        assert!(
            over.body["error"]
                .as_str()
                .unwrap()
                .contains("more of the installment")
        );
        let all = send(&app, get_request(&settlements, Some(&cookie))).await;
        assert_eq!(all.body.as_array().unwrap().len(), 2);

        // While it has settlements, its installments are fixed (409), but a description is not.
        let put = |body: Value| send(&app, json_request("PUT", &entry, Some(&cookie), body));
        let mut changed = entry_body(
            &targets,
            "Venda de bolos",
            &[("2026-12-10", "100"), ("2027-01-10", "50.5")],
        );
        assert_eq!(put(changed.clone()).await.status, StatusCode::OK);
        changed["installments"] = json!([{ "due_date": "2026-12-10", "amount": "150.5" }]);
        let fixed = put(changed).await;
        assert_eq!(fixed.status, StatusCode::CONFLICT);
        assert!(
            fixed.body["error"]
                .as_str()
                .unwrap()
                .contains("reverse them first")
        );

        // Reversing: 204, it stops counting and stays listed; once more is a 404.
        let id = rest.body["id"].as_str().unwrap().to_owned();
        let one = format!("{settlements}/{id}");
        let reversed = send(
            &app,
            json_request("DELETE", &one, Some(&cookie), Value::Null),
        )
        .await;
        assert_eq!(reversed.status, StatusCode::NO_CONTENT);
        assert_eq!(
            send(
                &app,
                json_request("DELETE", &one, Some(&cookie), Value::Null)
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
        let after = send(&app, get_request(&entry, Some(&cookie))).await;
        assert_eq!(after.body["installments"][0]["settled"], "40");
        let all = send(&app, get_request(&settlements, Some(&cookie))).await;
        let flags: Vec<bool> = all
            .body
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["reversed"].as_bool().unwrap())
            .collect();
        assert_eq!(flags.iter().filter(|reversed| **reversed).count(), 1);
        assert_eq!(settle(body(1, "60")).await.status, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn a_settlement_that_points_at_what_cannot_be_used_or_is_not_money_is_refused_in_json() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let account = registered_account(&app, &cookie, company_id, "Caixa").await;
        let created = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&cookie),
                entry_body(&targets, "Venda", &[("2026-12-10", "100")]),
            ),
        )
        .await;
        let settlements = format!(
            "{}/{}/settlements",
            entries_uri(company_id),
            created.body["id"].as_str().unwrap()
        );
        let ok = json!({ "installment": 1, "settled_on": "2026-12-10", "applied": "10", "bank_account_id": account });
        let post = |body: Value| {
            send(
                &app,
                json_request("POST", &settlements, Some(&cookie), body),
            )
        };
        let with = |field: &str, value: Value| {
            let mut body = ok.clone();
            body[field] = value;
            body
        };

        for (body, status, what) in [
            (
                with("installment", json!(2)),
                StatusCode::UNPROCESSABLE_ENTITY,
                "installment",
            ),
            (
                with("bank_account_id", json!(Uuid::new_v4())),
                StatusCode::UNPROCESSABLE_ENTITY,
                "bank account",
            ),
            (
                with("payment_method_id", json!(Uuid::new_v4())),
                StatusCode::UNPROCESSABLE_ENTITY,
                "payment method",
            ),
            (
                with("applied", json!("0")),
                StatusCode::UNPROCESSABLE_ENTITY,
                "applied",
            ),
            (
                with("applied", json!("1.005")),
                StatusCode::UNPROCESSABLE_ENTITY,
                "applied",
            ),
            (
                with("interest", json!("-1")),
                StatusCode::UNPROCESSABLE_ENTITY,
                "interest",
            ),
            (
                with("discount", json!("10")),
                StatusCode::UNPROCESSABLE_ENTITY,
                "must move some money",
            ),
            (
                with("settled_on", json!("10/12/2026")),
                StatusCode::UNPROCESSABLE_ENTITY,
                "settled_on",
            ),
            (
                with("installment", json!(0)),
                StatusCode::UNPROCESSABLE_ENTITY,
                "installment",
            ),
            (
                with("applied", json!(10)),
                StatusCode::UNPROCESSABLE_ENTITY,
                "",
            ),
            (
                with("cash", json!("10")),
                StatusCode::UNPROCESSABLE_ENTITY,
                "",
            ),
        ] {
            let reply = post(body.clone()).await;
            assert_eq!(reply.status, status, "{body}");
            assert!(
                reply.body["error"].as_str().unwrap().contains(what),
                "{}",
                reply.body
            );
        }
        let none = send(&app, get_request(&settlements, Some(&cookie))).await;
        assert_eq!(
            none.body.as_array().unwrap().len(),
            0,
            "a refused settlement is not stored"
        );

        // An id that is not one is a JSON 404, and so is an entry that is not there.
        let bad = send(
            &app,
            json_request(
                "DELETE",
                &format!("{settlements}/not-an-id"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(bad.status, StatusCode::NOT_FOUND);
        let missing = send(
            &app,
            get_request(
                &format!("{}/{}/settlements", entries_uri(company_id), Uuid::new_v4()),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(missing.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn settlements_stay_private_to_the_company() {
        let app = entry_app();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let targets = entry_targets(&app, &owner, company_id).await;
        let account = registered_account(&app, &owner, company_id, "Caixa").await;
        let created = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&owner),
                entry_body(&targets, "Venda", &[("2026-12-10", "100")]),
            ),
        )
        .await;
        let settlements = format!(
            "{}/{}/settlements",
            entries_uri(company_id),
            created.body["id"].as_str().unwrap()
        );
        let body = json!({ "installment": 1, "settled_on": "2026-12-10", "applied": "10", "bank_account_id": account });
        let settled = send(
            &app,
            json_request("POST", &settlements, Some(&owner), body.clone()),
        )
        .await;
        assert_eq!(settled.status, StatusCode::CREATED);
        let one = format!("{settlements}/{}", settled.body["id"].as_str().unwrap());

        // Another company's member does not learn it exists.
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        for request in [
            get_request(&settlements, Some(&stranger)),
            json_request("POST", &settlements, Some(&stranger), body.clone()),
            json_request("DELETE", &one, Some(&stranger), Value::Null),
        ] {
            assert_eq!(send(&app, request).await.status, StatusCode::NOT_FOUND);
        }
        assert_eq!(
            send(&app, get_request(&settlements, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    /// Chooses the account charges are paid into (and the category of the provider's fee), which a
    /// charge needs before it can be issued. Answers the account's id.
    async fn configure_collection(
        app: &Router,
        cookie: &str,
        company_id: Uuid,
        fee_category: Option<&str>,
    ) -> String {
        let account = registered_account(app, cookie, company_id, "Conta da cobrança").await;
        let settings = send(
            app,
            json_request(
                "PUT",
                &format!("/companies/{company_id}/collection/settings"),
                Some(cookie),
                json!({ "bank_account_id": account, "fee_category_id": fee_category }),
            ),
        )
        .await;
        assert_eq!(settings.status, StatusCode::OK, "{}", settings.body);
        account
    }

    #[tokio::test]
    async fn issues_lists_and_cancels_charges_for_an_installment() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let created = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&cookie),
                entry_body(
                    &targets,
                    "Venda",
                    &[("2026-12-10", "100"), ("2027-01-10", "50.50")],
                ),
            ),
        )
        .await;
        let entry_id = created.body["id"].as_str().unwrap().to_owned();
        let uri = format!("/companies/{company_id}/entries/{entry_id}/charges");

        // Without the account the money lands in, nothing is asked of the provider.
        let refused = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                json!({ "installment": 1, "method": "pix" }),
            ),
        )
        .await;
        assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.body);
        configure_collection(&app, &cookie, company_id, None).await;

        let collection = send(
            &app,
            get_request(
                &format!("/companies/{company_id}/collection"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(collection.body["provider"], "fake");

        let issued = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                json!({ "installment": 1, "method": "pix" }),
            ),
        )
        .await;
        assert_eq!(issued.status, StatusCode::CREATED, "{}", issued.body);
        assert_eq!(issued.body["status"], "open");
        assert_eq!(issued.body["amount"], "100");
        assert_eq!(issued.body["due_on"], "2026-12-10");
        assert_eq!(issued.body["provider"], "fake");
        assert!(issued.body["pix_payload"].as_str().is_some());
        assert!(issued.body["digitable_line"].is_null());
        assert_eq!(issued.body["issued_by"], "Ana");
        let charge_uri = format!("{uri}/{}", issued.body["id"].as_str().unwrap());

        // A second one for the same installment is a conflict; another installment and method are fine.
        let again = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                json!({ "installment": 1, "method": "boleto" }),
            ),
        )
        .await;
        assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.body);
        let boleto = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                json!({ "installment": 2, "method": "boleto", "due_on": "2027-01-15" }),
            ),
        )
        .await;
        assert_eq!(boleto.status, StatusCode::CREATED, "{}", boleto.body);
        assert_eq!(boleto.body["due_on"], "2027-01-15");
        assert!(boleto.body["digitable_line"].as_str().is_some());
        let listed = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 2);

        // What is refused says why: a method it does not know, a date that is not one, an installment it lacks.
        for (body, status) in [
            (
                json!({ "installment": 1, "method": "cash" }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "installment": 2, "method": "pix", "due_on": "tomorrow" }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "installment": 9, "method": "pix" }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "installment": 1, "method": "pix", "extra": true }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            assert_eq!(
                send(&app, json_request("POST", &uri, Some(&cookie), body))
                    .await
                    .status,
                status
            );
        }

        let cancelled = send(
            &app,
            json_request(
                "POST",
                &format!("{charge_uri}/cancel"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.body);
        assert_eq!(cancelled.body["status"], "cancelled");
        let twice = send(
            &app,
            json_request(
                "POST",
                &format!("{charge_uri}/cancel"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(twice.status, StatusCode::CONFLICT);
        let history = send(
            &app,
            get_request(&format!("{charge_uri}/history"), Some(&cookie)),
        )
        .await;
        assert_eq!(history.body.as_array().unwrap().len(), 2);
        assert_eq!(history.body[0]["action"], "created");
    }

    #[tokio::test]
    async fn charges_stay_private_to_the_company_and_need_a_session() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let created = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&cookie),
                entry_body(&targets, "Venda", &[("2026-12-10", "100")]),
            ),
        )
        .await;
        let uri = format!(
            "/companies/{company_id}/entries/{}/charges",
            created.body["id"].as_str().unwrap()
        );
        configure_collection(&app, &cookie, company_id, None).await;
        let body = json!({ "installment": 1, "method": "pix" });
        let issued = send(
            &app,
            json_request("POST", &uri, Some(&cookie), body.clone()),
        )
        .await;
        let one = format!("{uri}/{}", issued.body["id"].as_str().unwrap());

        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        for request in [
            get_request(&uri, Some(&stranger)),
            json_request("POST", &uri, Some(&stranger), body.clone()),
            json_request(
                "POST",
                &format!("{one}/cancel"),
                Some(&stranger),
                Value::Null,
            ),
            get_request(&format!("{one}/history"), Some(&stranger)),
            get_request(
                &format!("/companies/{company_id}/collection"),
                Some(&stranger),
            ),
            get_request(
                &format!("/companies/{company_id}/collection/settings"),
                Some(&stranger),
            ),
            json_request(
                "PUT",
                &format!("/companies/{company_id}/collection/settings"),
                Some(&stranger),
                json!({ "bank_account_id": Uuid::new_v4(), "fee_category_id": null }),
            ),
        ] {
            assert_eq!(send(&app, request).await.status, StatusCode::NOT_FOUND);
        }
        assert_eq!(
            send(&app, get_request(&uri, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    fn signed_notice(
        event: &str,
        external_id: &str,
        kind: &str,
        paid: Option<(&str, &str, &str)>,
    ) -> (String, Vec<u8>) {
        let (headers, body) = FakePaymentProvider::notice(event, external_id, kind, paid);
        let signature = headers
            .get(conta_verde_infrastructure::FAKE_SIGNATURE_HEADER)
            .unwrap()
            .to_owned();
        (signature, body)
    }

    fn webhook_request(uri: &str, signature: Option<&str>, body: Vec<u8>) -> Request<Body> {
        let mut builder = Request::builder().method("POST").uri(uri);
        if let Some(signature) = signature {
            builder = builder.header(conta_verde_infrastructure::FAKE_SIGNATURE_HEADER, signature);
        }
        builder.body(Body::from(body)).unwrap()
    }

    #[tokio::test]
    async fn a_signed_webhook_settles_the_installment_and_enters_the_fee() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let account =
            configure_collection(&app, &cookie, company_id, Some(&targets.other_category)).await;
        let created = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&cookie),
                entry_body(&targets, "Venda", &[("2026-12-10", "100")]),
            ),
        )
        .await;
        let entry_id = created.body["id"].as_str().unwrap().to_owned();
        let issued = send(
            &app,
            json_request(
                "POST",
                &format!("/companies/{company_id}/entries/{entry_id}/charges"),
                Some(&cookie),
                json!({ "installment": 1, "method": "pix" }),
            ),
        )
        .await;
        let external = format!(
            "fake_{}",
            issued.body["id"].as_str().unwrap().replace('-', "")
        );
        let hook = format!("/public/payments/{company_id}/webhooks/fake");

        // No session is needed, but the signature is: unsigned, forged and for another company all fail.
        let (signature, body) = signed_notice(
            "evt_1",
            &external,
            "paid",
            Some(("100", "2026-12-09", "1.99")),
        );
        for request in [
            webhook_request(&hook, None, body.clone()),
            webhook_request(&hook, Some(&"0".repeat(64)), body.clone()),
            webhook_request(&hook, Some(&signature), b"{}".to_vec()),
        ] {
            assert_eq!(send(&app, request).await.status, StatusCode::UNAUTHORIZED);
        }
        // Another company's address: the charge is not its own, so it is left out (and answered, so a provider
        // account with payments made elsewhere does not pause its queue); nothing is applied.
        let elsewhere = send(
            &app,
            webhook_request(
                &format!("/public/payments/{}/webhooks/fake", Uuid::new_v4()),
                Some(&signature),
                body.clone(),
            ),
        )
        .await;
        assert_eq!(elsewhere.status, StatusCode::OK);
        assert_eq!(elsewhere.body["received"], 0);
        for uri in [
            format!("/public/payments/{company_id}/webhooks/other"),
            "/public/payments/not-an-id/webhooks/fake".to_owned(),
        ] {
            assert_eq!(
                send(&app, webhook_request(&uri, Some(&signature), body.clone()))
                    .await
                    .status,
                StatusCode::NOT_FOUND,
                "{uri}"
            );
        }

        // Signed, it settles the installment; the same notice again says so and settles nothing more.
        for _ in 0..2 {
            let reply = send(&app, webhook_request(&hook, Some(&signature), body.clone())).await;
            assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
            assert_eq!(reply.body["received"], 1);
        }
        let settlements = send(
            &app,
            get_request(
                &format!("{}/{entry_id}/settlements", entries_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        let settlements = settlements.body.as_array().unwrap();
        assert_eq!(settlements.len(), 1);
        assert_eq!(settlements[0]["applied"], "100");
        assert_eq!(settlements[0]["bank_account_id"], account);
        let charges = send(
            &app,
            get_request(
                &format!("/companies/{company_id}/entries/{entry_id}/charges"),
                Some(&cookie),
            ),
        )
        .await;
        let charge = &charges.body["items"][0];
        assert_eq!(charge["status"], "paid");
        assert_eq!(charge["paid_on"], "2026-12-09");
        assert_eq!(charge["paid_amount"], "100");
        assert_eq!(charge["fee"], "1.99");
        assert!(charge["note"].is_null());
        // The fee is an expense settled from the same account.
        let entries = send(&app, get_request(&entries_uri(company_id), Some(&cookie))).await;
        let fee = entries.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| {
                entry["description"]
                    .as_str()
                    .unwrap()
                    .starts_with("Taxa de cobrança")
            })
            .expect("the fee is an entry");
        assert_eq!(fee["kind"], "payable");
        assert_eq!(fee["total"], "1.99");
        assert_eq!(fee["installments"][0]["settled"], "1.99");

        // Settings are readable and are the company's alone.
        let settings = send(
            &app,
            get_request(
                &format!("/companies/{company_id}/collection/settings"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(settings.body["bank_account_id"], account);
        assert_eq!(settings.body["fee_category_id"], targets.other_category);
    }

    #[tokio::test]
    async fn a_customer_opens_the_payment_page_from_a_link_with_no_session() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        configure_collection(&app, &cookie, company_id, None).await;
        let created = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&cookie),
                entry_body(&targets, "Venda 12", &[("2026-12-10", "100")]),
            ),
        )
        .await;
        let entry_id = created.body["id"].as_str().unwrap().to_owned();
        let charges = format!("/companies/{company_id}/entries/{entry_id}/charges");
        let issued = send(
            &app,
            json_request(
                "POST",
                &charges,
                Some(&cookie),
                json!({ "installment": 1, "method": "pix" }),
            ),
        )
        .await;
        let charge_uri = format!("{charges}/{}", issued.body["id"].as_str().unwrap());

        // The person makes a link; the address holds the token, which is not kept.
        let made = send(
            &app,
            json_request(
                "POST",
                &format!("{charge_uri}/link"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
        let url = made.body["url"].as_str().unwrap();
        let token = url.strip_prefix("https://app.example/pay/").expect(url);
        let public = format!("/public/charges/{token}");

        // The customer needs no session, and sees what to pay with, and nothing of the books.
        let page = send(&app, get_request(&public, None)).await;
        assert_eq!(page.status, StatusCode::OK, "{}", page.body);
        assert_eq!(page.body["state"], "payable");
        assert_eq!(page.body["description"], "Venda 12");
        assert_eq!(page.body["amount"], "100");
        assert_eq!(page.body["method"], "pix");
        assert_eq!(page.body["installments"], 1);
        assert_eq!(page.body["pix_payload"], issued.body["pix_payload"]);
        for secret in [
            "id",
            "company_id",
            "entry_id",
            "external_id",
            "provider",
            "issued_by",
        ] {
            assert!(page.body.get(secret).is_none(), "{secret}");
        }
        assert_eq!(
            send(&app, get_request("/public/charges/not-a-token", None))
                .await
                .status,
            StatusCode::NOT_FOUND
        );

        // Nobody else's session makes a link for it.
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        assert_eq!(
            send(
                &app,
                json_request(
                    "POST",
                    &format!("{charge_uri}/link"),
                    Some(&stranger),
                    Value::Null
                )
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );

        // Once it is paid the page says so and offers nothing to pay with.
        let external = format!(
            "fake_{}",
            issued.body["id"].as_str().unwrap().replace('-', "")
        );
        let (signature, body) = signed_notice(
            "evt_page",
            &external,
            "paid",
            Some(("100", "2026-12-09", "0")),
        );
        let hook = format!("/public/payments/{company_id}/webhooks/fake");
        assert_eq!(
            send(&app, webhook_request(&hook, Some(&signature), body))
                .await
                .status,
            StatusCode::OK
        );
        let paid = send(&app, get_request(&public, None)).await;
        assert_eq!(paid.body["state"], "paid");
        assert_eq!(paid.body["paid_on"], "2026-12-09");
        assert!(paid.body["pix_payload"].is_null());
        // A paid charge gets no new link.
        assert_eq!(
            send(
                &app,
                json_request(
                    "POST",
                    &format!("{charge_uri}/link"),
                    Some(&cookie),
                    Value::Null
                )
            )
            .await
            .status,
            StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn the_reminder_plan_is_replaced_as_a_whole_audited_and_private() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = format!("/companies/{company_id}/collection/dunning");

        // Never made: off, no rules; WhatsApp is available in these tests.
        let fresh = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(fresh.status, StatusCode::OK);
        assert_eq!(fresh.body["enabled"], false);
        assert_eq!(fresh.body["rules"], json!([]));
        assert_eq!(fresh.body["whatsapp_available"], true);

        // The rules come back in the order they fall.
        let saved = send(
            &app,
            json_request(
                "PUT",
                &uri,
                Some(&cookie),
                json!({ "enabled": true, "rules": [
                    { "days": 3, "channel": "email" },
                    { "days": -3, "channel": "whatsapp" },
                    { "days": -3, "channel": "email" },
                ]}),
            ),
        )
        .await;
        assert_eq!(saved.status, StatusCode::OK, "{}", saved.body);
        assert_eq!(saved.body["enabled"], true);
        assert_eq!(
            saved.body["rules"],
            json!([
                { "days": -3, "channel": "email" },
                { "days": -3, "channel": "whatsapp" },
                { "days": 3, "channel": "email" },
            ])
        );
        assert_eq!(
            send(&app, get_request(&uri, Some(&cookie))).await.body,
            saved.body
        );
        let history = send(&app, get_request(&format!("{uri}/history"), Some(&cookie))).await;
        assert_eq!(history.body.as_array().unwrap().len(), 1);

        // What is refused says why.
        for body in [
            json!({ "enabled": true, "rules": [{ "days": 0, "channel": "sms" }] }),
            json!({ "enabled": true, "rules": [{ "days": 999, "channel": "email" }] }),
            json!({ "enabled": true, "rules": [
                { "days": 1, "channel": "email" }, { "days": 1, "channel": "email" }] }),
            json!({ "rules": [] }),
        ] {
            assert_eq!(
                send(&app, json_request("PUT", &uri, Some(&cookie), body))
                    .await
                    .status,
                StatusCode::UNPROCESSABLE_ENTITY
            );
        }

        // Another company's member sees nothing of it, and a session is needed.
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        for request in [
            get_request(&uri, Some(&stranger)),
            json_request(
                "PUT",
                &uri,
                Some(&stranger),
                json!({ "enabled": false, "rules": [] }),
            ),
            get_request(&format!("{uri}/history"), Some(&stranger)),
            get_request(
                &format!(
                    "/companies/{company_id}/entries/{}/notifications",
                    Uuid::new_v4()
                ),
                Some(&cookie),
            ),
        ] {
            assert_eq!(send(&app, request).await.status, StatusCode::NOT_FOUND);
        }
        assert_eq!(
            send(&app, get_request(&uri, None)).await.status,
            StatusCode::UNAUTHORIZED
        );

        // An entry with no reminders yet lists none.
        let targets = entry_targets(&app, &cookie, company_id).await;
        let created = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&cookie),
                entry_body(&targets, "Venda", &[("2026-12-10", "100")]),
            ),
        )
        .await;
        let reminders = send(
            &app,
            get_request(
                &format!(
                    "/companies/{company_id}/entries/{}/notifications",
                    created.body["id"].as_str().unwrap()
                ),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(reminders.status, StatusCode::OK);
        assert_eq!(reminders.body["items"], json!([]));
    }

    #[tokio::test]
    async fn the_delinquency_report_adds_up_what_is_late_by_customer_and_range() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let uri = format!("/companies/{company_id}/delinquency?on=2027-01-31");
        let empty = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(empty.status, StatusCode::OK);
        assert_eq!(empty.body["owed"], "0");
        assert_eq!(empty.body["customers"], json!([]));
        assert_eq!(empty.body["buckets"].as_array().unwrap().len(), 4);
        assert_eq!(empty.body["buckets"][0]["from_days"], 1);
        assert_eq!(empty.body["buckets"][0]["to_days"], 30);
        assert!(empty.body["buckets"][3]["to_days"].is_null());

        for (due, amount) in [
            ("2027-01-20", "100"),
            ("2026-10-01", "10"),
            ("2027-02-10", "99"),
        ] {
            let made = send(
                &app,
                json_request(
                    "POST",
                    &entries_uri(company_id),
                    Some(&cookie),
                    entry_body(&targets, "Venda", &[(due, amount)]),
                ),
            )
            .await;
            assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
        }
        let report = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(report.body["on"], "2027-01-31");
        assert_eq!(report.body["owed"], "110");
        assert_eq!(report.body["installments"], 2);
        assert_eq!(report.body["buckets"][0]["owed"], "100");
        assert_eq!(report.body["buckets"][0]["installments"], 1);
        assert_eq!(report.body["buckets"][3]["owed"], "10");
        let customer = &report.body["customers"][0];
        assert_eq!(customer["party_id"], targets.party);
        assert_eq!(customer["owed"], "110");
        assert_eq!(customer["oldest_due"], "2026-10-01");
        assert_eq!(customer["by_bucket"], json!(["100", "0", "0", "10"]));

        // The day is required and must be one; another company sees nothing of it, and a session is needed.
        for bad in [
            format!("/companies/{company_id}/delinquency"),
            format!("/companies/{company_id}/delinquency?on=tomorrow"),
            format!("/companies/{company_id}/delinquency?on=2027-01-31&x=1"),
        ] {
            assert_eq!(
                send(&app, get_request(&bad, Some(&cookie))).await.status,
                StatusCode::BAD_REQUEST,
                "{bad}"
            );
        }
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        assert_eq!(
            send(&app, get_request(&uri, Some(&stranger))).await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&uri, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    fn attachments_uri(company_id: Uuid, entry_id: &str) -> String {
        format!("/companies/{company_id}/entries/{entry_id}/attachments")
    }

    fn file_request(
        method: &str,
        uri: &str,
        cookie: Option<&str>,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, content_type);
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        builder.body(Body::from(bytes)).unwrap()
    }

    #[tokio::test]
    async fn attaches_lists_downloads_and_removes_a_file() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let created = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&cookie),
                entry_body(&targets, "Venda", &[("2026-12-10", "100")]),
            ),
        )
        .await;
        let entry_id = created.body["id"].as_str().unwrap().to_owned();
        let uri = attachments_uri(company_id, &entry_id);

        let attached = send(
            &app,
            file_request(
                "POST",
                &format!("{uri}?name=Nota%20123%20%C3%A7.pdf"),
                Some(&cookie),
                "application/pdf; charset=binary",
                b"%PDF-1.4 nota".to_vec(),
            ),
        )
        .await;
        assert_eq!(attached.status, StatusCode::CREATED, "{}", attached.body);
        assert_eq!(attached.body["file_name"], "Nota 123 ç.pdf");
        assert_eq!(attached.body["content_type"], "application/pdf");
        assert_eq!(attached.body["size"], 13);
        assert_eq!(attached.body["entry_id"], entry_id);
        let one = format!("{uri}/{}", attached.body["id"].as_str().unwrap());

        let listed = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(listed.body["items"][0]["file_name"], "Nota 123 ç.pdf");

        // A download is the bytes, as a download, never sniffed as something else.
        let response = app
            .clone()
            .oneshot(get_request(&one, Some(&cookie)))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers().clone();
        assert_eq!(headers[header::CONTENT_TYPE], "application/pdf");
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(
            headers[header::CONTENT_DISPOSITION],
            "attachment; filename=\"Nota 123 _.pdf\"; filename*=UTF-8''Nota%20123%20%C3%A7.pdf"
        );
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..], b"%PDF-1.4 nota");

        let removed = send(
            &app,
            json_request("DELETE", &one, Some(&cookie), Value::Null),
        )
        .await;
        assert_eq!(removed.status, StatusCode::NO_CONTENT);
        let after = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(after.body["items"], json!([]));
        assert_eq!(
            send(&app, get_request(&one, Some(&cookie))).await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(
                &app,
                json_request("DELETE", &one, Some(&cookie), Value::Null)
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn a_file_that_is_not_taken_is_refused_in_json() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let created = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&cookie),
                entry_body(&targets, "Venda", &[("2026-12-10", "100")]),
            ),
        )
        .await;
        let uri = attachments_uri(company_id, created.body["id"].as_str().unwrap());
        let attach = |name: &str, kind: &str, bytes: Vec<u8>| {
            send(
                &app,
                file_request(
                    "POST",
                    &format!("{uri}?name={name}"),
                    Some(&cookie),
                    kind,
                    bytes,
                ),
            )
        };

        for (name, kind, bytes, status, what) in [
            (
                "a.html",
                "text/html",
                b"<b>".to_vec(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "file type",
            ),
            (
                "a.pdf",
                "application/pdf",
                Vec::new(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "file must have",
            ),
            (
                "a%2Fb.pdf",
                "application/pdf",
                b"x".to_vec(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "file name",
            ),
            (
                "big.pdf",
                "application/pdf",
                vec![0; 10 * 1024 * 1024 + 1],
                StatusCode::PAYLOAD_TOO_LARGE,
                "larger than 10 MB",
            ),
        ] {
            let reply = attach(name, kind, bytes).await;
            assert_eq!(reply.status, status, "{name}: {}", reply.body);
            assert!(
                reply.body["error"].as_str().unwrap().contains(what),
                "{name}: {}",
                reply.body
            );
        }
        // The largest file is taken.
        let largest = attach("max.pdf", "application/pdf", vec![0; 10 * 1024 * 1024]).await;
        assert_eq!(largest.status, StatusCode::CREATED, "{}", largest.body);
        // The name is needed, and nothing else is.
        let nameless = send(
            &app,
            file_request(
                "POST",
                &uri,
                Some(&cookie),
                "application/pdf",
                b"x".to_vec(),
            ),
        )
        .await;
        assert_eq!(nameless.status, StatusCode::BAD_REQUEST);
        let listed = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn attachments_stay_private_to_the_company() {
        let app = entry_app();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let targets = entry_targets(&app, &owner, company_id).await;
        let created = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&owner),
                entry_body(&targets, "Venda", &[("2026-12-10", "100")]),
            ),
        )
        .await;
        let entry_id = created.body["id"].as_str().unwrap().to_owned();
        let uri = attachments_uri(company_id, &entry_id);
        let attached = send(
            &app,
            file_request(
                "POST",
                &format!("{uri}?name=Nota.pdf"),
                Some(&owner),
                "application/pdf",
                b"x".to_vec(),
            ),
        )
        .await;
        let one = format!("{uri}/{}", attached.body["id"].as_str().unwrap());

        // Another company's member does not learn it exists, and a stranger to the entry finds nothing.
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        for request in [
            get_request(&uri, Some(&stranger)),
            get_request(&one, Some(&stranger)),
            file_request(
                "POST",
                &format!("{uri}?name=A.pdf"),
                Some(&stranger),
                "application/pdf",
                b"x".to_vec(),
            ),
            json_request("DELETE", &one, Some(&stranger), Value::Null),
        ] {
            assert_eq!(send(&app, request).await.status, StatusCode::NOT_FOUND);
        }
        for request in [
            get_request(
                &format!("{}/not-an-id", attachments_uri(company_id, "x")),
                Some(&owner),
            ),
            get_request(&format!("{uri}/not-an-id"), Some(&owner)),
            get_request(
                &format!(
                    "{}/{}",
                    attachments_uri(company_id, &Uuid::new_v4().to_string()),
                    Uuid::new_v4()
                ),
                Some(&owner),
            ),
        ] {
            assert_eq!(send(&app, request).await.status, StatusCode::NOT_FOUND);
        }
        assert_eq!(
            send(&app, get_request(&one, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    fn payment_terms_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/payment-terms")
    }

    #[tokio::test]
    async fn registers_and_changes_payment_terms_whose_percentages_add_up() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = payment_terms_uri(company_id);
        let body = json!({
            "name": " 30/60/90 ",
            "installments": [
                { "days": 30, "percent": "33.33" },
                { "days": 60, "percent": "33.33" },
                { "days": 90, "percent": "33.34" }
            ]
        });

        let created = send(
            &app,
            json_request("POST", &uri, Some(&cookie), body.clone()),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED);
        assert_eq!(created.body["name"], "30/60/90");
        assert_eq!(created.body["active"], true);
        assert_eq!(
            created.body["installments"][2],
            json!({ "days": 90, "percent": "33.34" })
        );
        let id = created.body["id"].as_str().unwrap().to_owned();
        let duplicate = send(&app, json_request("POST", &uri, Some(&cookie), body)).await;
        assert_eq!(duplicate.status, StatusCode::CONFLICT);

        // The whole term is replaced; whole percentages come back as written without zeros.
        let term_uri = format!("{uri}/{id}");
        let change = json!({
            "name": "Entrada e 30",
            "installments": [{ "days": 0, "percent": "50.00" }, { "days": 30, "percent": "50" }],
            "active": false
        });
        let updated = send(
            &app,
            json_request("PUT", &term_uri, Some(&cookie), change.clone()),
        )
        .await;
        assert_eq!(updated.status, StatusCode::OK);
        assert_eq!(
            updated.body["installments"],
            json!([{ "days": 0, "percent": "50" }, { "days": 30, "percent": "50" }])
        );
        // The same body again changes nothing and adds nothing to the history.
        send(&app, json_request("PUT", &term_uri, Some(&cookie), change)).await;

        let listed = send(
            &app,
            get_request(
                &format!("{uri}?active=false&q=ENTRADA&sort=-name"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(
            listed.body["items"][0]["installments"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let history = send(
            &app,
            get_request(&format!("{term_uri}/history"), Some(&cookie)),
        )
        .await;
        let entries = history.body.as_array().unwrap();
        assert_eq!(entries.len(), 2);
        let changes = entries[1]["changes"].as_array().unwrap();
        assert_eq!(
            changes[1],
            json!({ "field": "installments", "from": "30:33.33,60:33.33,90:33.34", "to": "0:50,30:50" })
        );
    }

    #[tokio::test]
    async fn payment_terms_that_do_not_add_up_are_refused_in_json() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let uri = payment_terms_uri(company_id);
        let named = |installments: Value| json!({ "name": "Prazo", "installments": installments });
        let refusals: [(Value, &str); 8] = [
            (named(json!([])), "at least one installment"),
            (
                named(json!([{ "days": 0, "percent": "99.99" }])),
                "add up to exactly 100",
            ),
            (
                named(json!([{ "days": 0, "percent": "50" }, { "days": 30, "percent": "50.01" }])),
                "add up to exactly 100",
            ),
            (
                named(json!([{ "days": 30, "percent": "50" }, { "days": 30, "percent": "50" }])),
                "must increase",
            ),
            (
                named(json!([{ "days": 60, "percent": "50" }, { "days": 30, "percent": "50" }])),
                "must increase",
            ),
            (
                named(json!([{ "days": 0, "percent": "50,5" }, { "days": 30, "percent": "49.5" }])),
                "percentage must be",
            ),
            (
                named(json!([{ "days": 0, "percent": "33.333" }])),
                "percentage must be",
            ),
            (
                named(json!([{ "days": 3651, "percent": "100" }])),
                "0 to 3650 days",
            ),
        ];
        for (body, message) in refusals {
            let reply = send(
                &app,
                json_request("POST", &uri, Some(&cookie), body.clone()),
            )
            .await;
            assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert!(
                reply.body["error"].as_str().unwrap().contains(message),
                "{body}: {}",
                reply.body
            );
        }
        // A percentage sent as a JSON number would lose precision: refused in the API's own shape.
        for body in [
            named(json!([{ "days": 0, "percent": 100 }])),
            named(json!([{ "days": -1, "percent": "100" }])),
            named(json!([{ "days": 0, "percent": "100", "surprise": 1 }])),
            json!({ "name": "Prazo" }),
            json!({ "installments": [{ "days": 0, "percent": "100" }] }),
        ] {
            let reply = send(
                &app,
                json_request("POST", &uri, Some(&cookie), body.clone()),
            )
            .await;
            assert!(reply.status.is_client_error(), "{body}");
            assert!(reply.body["error"].is_string(), "{body}");
        }
        // More than 24 installments, and a name too long.
        let many: Vec<Value> = (1..=25)
            .map(|day| json!({ "days": day, "percent": "4" }))
            .collect();
        let reply = send(
            &app,
            json_request("POST", &uri, Some(&cookie), named(Value::Array(many))),
        )
        .await;
        assert!(reply.body["error"].as_str().unwrap().contains("at most 24"));
        let long =
            json!({ "name": "x".repeat(201), "installments": [{ "days": 0, "percent": "100" }] });
        let reply = send(&app, json_request("POST", &uri, Some(&cookie), long)).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        // Nothing of that was stored.
        let listed = send(&app, get_request(&uri, Some(&cookie))).await;
        assert!(listed.body["items"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn payment_terms_stay_private_and_must_say_whether_they_are_active_when_replaced() {
        let app = app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let (stranger_cookie, stranger_company) = sign_up(&app, "bia@example.com").await;
        let uri = payment_terms_uri(company_id);
        let body = json!({ "name": "À vista", "installments": [{ "days": 0, "percent": "100" }], "active": false });
        let created = send(
            &app,
            json_request("POST", &uri, Some(&cookie), body.clone()),
        )
        .await;
        assert_eq!(created.body["active"], false);
        let id = created.body["id"].as_str().unwrap().to_owned();

        // Replacing without `active` would switch it back on.
        let mut without = body.clone();
        without.as_object_mut().unwrap().remove("active");
        let put = json_request("PUT", &format!("{uri}/{id}"), Some(&cookie), without);
        let reply = send(&app, put).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            reply.body["error"]
                .as_str()
                .unwrap()
                .contains("active is required")
        );

        // Another company has its own and cannot reach this one's.
        let theirs = payment_terms_uri(stranger_company);
        let own = send(
            &app,
            json_request("POST", &theirs, Some(&stranger_cookie), body.clone()),
        )
        .await;
        assert_eq!(own.status, StatusCode::CREATED);
        let put = json_request(
            "PUT",
            &format!("{theirs}/{id}"),
            Some(&stranger_cookie),
            body.clone(),
        );
        assert_eq!(send(&app, put).await.status, StatusCode::NOT_FOUND);
        let history = get_request(&format!("{theirs}/{id}/history"), Some(&stranger_cookie));
        assert_eq!(send(&app, history).await.status, StatusCode::NOT_FOUND);
        assert_eq!(
            send(&app, get_request(&uri, Some(&stranger_cookie)))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&uri, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
        let malformed = json_request("PUT", &format!("{uri}/not-a-uuid"), Some(&cookie), body);
        let reply = send(&app, malformed).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND);
        assert_eq!(reply.body["error"], "payment term not found");
        for query in ["active=yes", "sort=id", "limit=abc", "after=garbage"] {
            let reply = send(&app, get_request(&format!("{uri}?{query}"), Some(&cookie))).await;
            assert!(reply.status.is_client_error(), "{query}");
            assert!(reply.body["error"].is_string(), "{query}");
        }
    }

    fn transfers_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/transfers")
    }

    fn balances_uri(company_id: Uuid, on: &str) -> String {
        format!("/companies/{company_id}/balances?on={on}")
    }

    #[tokio::test]
    async fn moves_money_between_accounts_and_reverses_it_through_the_api() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let caixa = registered_account(&app, &cookie, company_id, "Caixa").await;
        let banco = registered_account(&app, &cookie, company_id, "Banco").await;
        let uri = transfers_uri(company_id);
        let body = |from: &str, to: &str, amount: &str, day: &str| json!({ "from_account_id": from, "to_account_id": to, "amount": amount, "occurred_on": day });

        let created = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                json!({
                    "from_account_id": caixa, "to_account_id": banco, "amount": "250.50",
                    "occurred_on": "2026-12-10", "note": " Depósito ",
                }),
            ),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        assert_eq!(created.body["amount"], "250.5");
        assert_eq!(created.body["note"], "Depósito");
        assert_eq!(created.body["reversed"], false);
        assert_eq!(created.body["from_account_id"], caixa);
        let id = created.body["id"].as_str().unwrap().to_owned();
        let later = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                body(&banco, &caixa, "10", "2026-12-12"),
            ),
        )
        .await;
        assert_eq!(later.status, StatusCode::CREATED);

        // Newest first; narrowed by an account on either side; paged by an opaque cursor.
        let listed = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(listed.status, StatusCode::OK);
        assert_eq!(listed.body["items"][0]["id"], later.body["id"]);
        assert_eq!(listed.body["items"][1]["id"], id);
        let narrowed = send(
            &app,
            get_request(&format!("{uri}?account={caixa}"), Some(&cookie)),
        )
        .await;
        assert_eq!(narrowed.body["items"].as_array().unwrap().len(), 2);
        let first_page = send(&app, get_request(&format!("{uri}?limit=1"), Some(&cookie))).await;
        assert_eq!(first_page.body["items"].as_array().unwrap().len(), 1);
        let cursor = first_page.body["next_cursor"].as_str().unwrap().to_owned();
        let second_page = send(
            &app,
            get_request(&format!("{uri}?limit=1&after={cursor}"), Some(&cookie)),
        )
        .await;
        assert_eq!(second_page.body["items"][0]["id"], id);
        assert_eq!(second_page.body["next_cursor"], Value::Null);
        for query in ["account=abc", "after=garbage", "after=x|2026-01-01"] {
            let reply = send(&app, get_request(&format!("{uri}?{query}"), Some(&cookie))).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{query}");
            assert!(reply.body["error"].is_string(), "{query}");
        }

        // What is refused is JSON and stores nothing: the same account twice, an amount that is not
        // positive cents or is a JSON number, a date that is not one, an account of another company.
        let (stranger, other_company) = sign_up(&app, "bia@example.com").await;
        let alien = registered_account(&app, &stranger, other_company, "Outra").await;
        for (request, status) in [
            (
                body(&caixa, &caixa, "1", "2026-12-10"),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                body(&caixa, &banco, "0", "2026-12-10"),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                body(&caixa, &banco, "1.005", "2026-12-10"),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                body(&caixa, &banco, "1", "2026-02-30"),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                body(&caixa, &alien, "1", "2026-12-10"),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "from_account_id": caixa, "to_account_id": banco, "amount": 1, "occurred_on": "2026-12-10" }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "from_account_id": caixa, "to_account_id": banco, "amount": "1", "occurred_on": "2026-12-10", "extra": true }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            let reply = send(&app, json_request("POST", &uri, Some(&cookie), request)).await;
            assert_eq!(reply.status, status, "{}", reply.body);
            assert!(reply.body["error"].is_string());
        }
        let after_refusals = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(after_refusals.body["items"].as_array().unwrap().len(), 2);

        // Reversing: 204, it stays listed and flagged; once more, a malformed id and another
        // company's are all 404.
        let one = format!("{uri}/{id}");
        let delete = |target: &str, cookie: &str| {
            send(
                &app,
                json_request("DELETE", target, Some(cookie), Value::Null),
            )
        };
        assert_eq!(delete(&one, &stranger).await.status, StatusCode::NOT_FOUND);
        assert_eq!(delete(&one, &cookie).await.status, StatusCode::NO_CONTENT);
        assert_eq!(delete(&one, &cookie).await.status, StatusCode::NOT_FOUND);
        assert_eq!(
            delete(&format!("{uri}/not-an-id"), &cookie).await.status,
            StatusCode::NOT_FOUND
        );
        let flags = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(flags.body["items"][1]["reversed"], true);
        assert_eq!(flags.body["items"][0]["reversed"], false);
    }

    #[tokio::test]
    async fn gives_the_balance_of_every_account_and_the_total_through_the_api() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let caixa = registered_account(&app, &cookie, company_id, "Caixa").await;
        let banco = registered_account(&app, &cookie, company_id, "Banco").await;

        // An opening balance, signed, replaced as a whole, refused when it is not cents.
        let opening_uri = format!("/companies/{company_id}/bank-accounts/{caixa}/opening-balance");
        let set = |balance: &str, on: &str| {
            send(
                &app,
                json_request(
                    "PUT",
                    &opening_uri,
                    Some(&cookie),
                    json!({ "balance": balance, "on": on }),
                ),
            )
        };
        let opening = set("1000.50", "2026-12-01").await;
        assert_eq!(opening.status, StatusCode::OK, "{}", opening.body);
        assert_eq!(
            opening.body,
            json!({ "balance": "1000.5", "on": "2026-12-01" })
        );
        assert_eq!(set("-5", "2026-12-01").await.body["balance"], "-5");
        assert_eq!(
            set("1.001", "2026-12-01").await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            set("1", "2026-13-01").await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(set("1000.50", "2026-12-01").await.status, StatusCode::OK);
        let unknown = format!(
            "/companies/{company_id}/bank-accounts/{}/opening-balance",
            Uuid::new_v4()
        );
        let missing = send(
            &app,
            json_request(
                "PUT",
                &unknown,
                Some(&cookie),
                json!({ "balance": "1", "on": "2026-12-01" }),
            ),
        )
        .await;
        assert_eq!(missing.status, StatusCode::NOT_FOUND);

        // A settlement and a transfer move it.
        let sale = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&cookie),
                entry_body(&targets, "Venda", &[("2026-12-10", "100")]),
            ),
        )
        .await;
        let settled = send(
            &app,
            json_request(
                "POST",
                &format!(
                    "{}/{}/settlements",
                    entries_uri(company_id),
                    sale.body["id"].as_str().unwrap()
                ),
                Some(&cookie),
                json!({ "installment": 1, "settled_on": "2026-12-10", "applied": "100", "interest": "5", "bank_account_id": caixa }),
            ),
        )
        .await;
        assert_eq!(settled.status, StatusCode::CREATED, "{}", settled.body);
        let moved = send(
            &app,
            json_request(
                "POST",
                &transfers_uri(company_id),
                Some(&cookie),
                json!({ "from_account_id": caixa, "to_account_id": banco, "amount": "200.25", "occurred_on": "2026-12-15" }),
            ),
        )
        .await;
        assert_eq!(moved.status, StatusCode::CREATED);

        let balances = send(
            &app,
            get_request(&balances_uri(company_id, "2026-12-31"), Some(&cookie)),
        )
        .await;
        assert_eq!(balances.status, StatusCode::OK, "{}", balances.body);
        assert_eq!(balances.body["on"], "2026-12-31");
        assert_eq!(balances.body["total"], "1105.5");
        let accounts = balances.body["accounts"].as_array().unwrap();
        assert_eq!(accounts.len(), 2);
        let of = |id: &str| {
            accounts
                .iter()
                .find(|item| item["account"]["id"] == id)
                .unwrap()
        };
        assert_eq!(
            of(&caixa),
            &json!({
                "account": of(&caixa)["account"],
                "opening": { "balance": "1000.5", "on": "2026-12-01" },
                "received": "105", "paid": "0", "transfers_in": "0", "transfers_out": "200.25",
                "balance": "905.25",
            })
        );
        assert_eq!(of(&caixa)["account"]["name"], "Caixa");
        assert_eq!(of(&banco)["opening"], Value::Null);
        assert_eq!(of(&banco)["balance"], "200.25");

        // Before the opening day the balance of an account that has one is 0.
        let before = send(
            &app,
            get_request(&balances_uri(company_id, "2026-11-30"), Some(&cookie)),
        )
        .await;
        assert_eq!(before.body["total"], "0");

        // `on` is required and has to be a date; nothing of another company shows.
        for uri in [
            format!("/companies/{company_id}/balances"),
            balances_uri(company_id, "tomorrow"),
            balances_uri(company_id, "2026-02-30"),
        ] {
            let reply = send(&app, get_request(&uri, Some(&cookie))).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{uri}");
            assert!(reply.body["error"].is_string(), "{uri}");
        }
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        assert_eq!(
            send(
                &app,
                get_request(&balances_uri(company_id, "2026-12-31"), Some(&stranger))
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn transfers_and_balances_follow_the_finance_role() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        for (role, may_write) in [
            (Role::Admin, true),
            (Role::Financial, true),
            (Role::Sales, false),
            (Role::Stock, false),
            (Role::Accountant, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            let reads = send(
                &app,
                get_request(&balances_uri(company_id, "2026-12-31"), Some(&cookie)),
            )
            .await;
            assert_eq!(reads.status, StatusCode::OK, "{role:?} reads balances");
            let lists = send(&app, get_request(&transfers_uri(company_id), Some(&cookie))).await;
            assert_eq!(lists.status, StatusCode::OK, "{role:?} reads transfers");
            for (method, uri) in [
                ("POST", transfers_uri(company_id)),
                (
                    "PUT",
                    format!(
                        "/companies/{company_id}/bank-accounts/{}/opening-balance",
                        Uuid::new_v4()
                    ),
                ),
            ] {
                let write = send(&app, json_request(method, &uri, Some(&cookie), json!({}))).await;
                let expected = if may_write {
                    StatusCode::UNPROCESSABLE_ENTITY
                } else {
                    StatusCode::FORBIDDEN
                };
                assert_eq!(write.status, expected, "{role:?} {method} {uri}");
            }
            let reverse = send(
                &app,
                json_request(
                    "DELETE",
                    &format!("{}/{}", transfers_uri(company_id), Uuid::new_v4()),
                    Some(&cookie),
                    Value::Null,
                ),
            )
            .await;
            let expected = if may_write {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(reverse.status, expected, "{role:?} reverses");
        }
        assert_eq!(
            send(&app, get_request(&transfers_uri(company_id), None))
                .await
                .status,
            StatusCode::UNAUTHORIZED
        );
    }

    fn cash_flow_uri(company_id: Uuid, query: &str) -> String {
        format!("/companies/{company_id}/cash-flow?{query}")
    }

    #[tokio::test]
    async fn gives_the_cash_flow_by_period_with_the_forecast_through_the_api() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let caixa = registered_account(&app, &cookie, company_id, "Caixa").await;
        let banco = registered_account(&app, &cookie, company_id, "Banco").await;
        let put_opening = send(
            &app,
            json_request(
                "PUT",
                &format!("/companies/{company_id}/bank-accounts/{caixa}/opening-balance"),
                Some(&cookie),
                json!({ "balance": "1000", "on": "2026-12-01" }),
            ),
        )
        .await;
        assert_eq!(put_opening.status, StatusCode::OK);
        let sale = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&cookie),
                entry_body(
                    &targets,
                    "Venda",
                    &[("2026-12-10", "100"), ("2027-01-10", "50.5")],
                ),
            ),
        )
        .await;
        let settle = send(
            &app,
            json_request(
                "POST",
                &format!(
                    "{}/{}/settlements",
                    entries_uri(company_id),
                    sale.body["id"].as_str().unwrap()
                ),
                Some(&cookie),
                json!({ "installment": 1, "settled_on": "2026-12-10", "applied": "100", "interest": "5", "bank_account_id": caixa }),
            ),
        )
        .await;
        assert_eq!(settle.status, StatusCode::CREATED, "{}", settle.body);
        let moved = send(
            &app,
            json_request(
                "POST",
                &transfers_uri(company_id),
                Some(&cookie),
                json!({ "from_account_id": caixa, "to_account_id": banco, "amount": "200.25", "occurred_on": "2026-12-15" }),
            ),
        )
        .await;
        assert_eq!(moved.status, StatusCode::CREATED);

        // By month (the default), for every account: the forecast is what is still open.
        let months = send(
            &app,
            get_request(
                &cash_flow_uri(company_id, "from=2026-12-01&to=2027-01-31"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(months.status, StatusCode::OK, "{}", months.body);
        assert_eq!(months.body["granularity"], "month");
        assert_eq!(months.body["account_id"], Value::Null);
        assert_eq!(months.body["opening_balance"], "0");
        assert_eq!(
            months.body["periods"][0],
            json!({
                "start": "2026-12-01", "end": "2026-12-31",
                "received": "105", "paid": "0",
                "transfers_in": "200.25", "transfers_out": "200.25",
                "net": "105", "new_opening_balances": "1000", "closing_balance": "1105",
                "to_receive": "0", "to_pay": "0", "projected_balance": "1105",
            })
        );
        assert_eq!(months.body["periods"][1]["to_receive"], "50.5");
        assert_eq!(months.body["periods"][1]["projected_balance"], "1155.5");
        assert_eq!(
            months.body["past_due"],
            json!({ "to_receive": "0", "to_pay": "0" })
        );

        // What fell due before the span and is still open is told apart and counted by the projection.
        let later = send(
            &app,
            get_request(
                &cash_flow_uri(company_id, "from=2027-02-01&to=2027-02-28"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(
            later.body["past_due"],
            json!({ "to_receive": "50.5", "to_pay": "0" })
        );
        assert_eq!(later.body["periods"][0]["to_receive"], "0");
        assert_eq!(later.body["periods"][0]["projected_balance"], "1155.5");

        // By day, for one account: no forecast, and the transfer is told in and out.
        let days = send(
            &app,
            get_request(
                &cash_flow_uri(
                    company_id,
                    &format!("from=2026-12-14&to=2026-12-16&granularity=day&account={banco}"),
                ),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(days.status, StatusCode::OK, "{}", days.body);
        assert_eq!(days.body["account_id"], banco);
        assert_eq!(days.body["periods"].as_array().unwrap().len(), 3);
        assert_eq!(days.body["periods"][1]["transfers_in"], "200.25");
        assert_eq!(days.body["periods"][1]["closing_balance"], "200.25");
        assert_eq!(days.body["periods"][1]["to_receive"], Value::Null);
        assert_eq!(days.body["periods"][1]["projected_balance"], Value::Null);

        // What is not understood is a 400 in JSON, never ignored; an account of nobody is a 404.
        for query in [
            "to=2026-12-31",
            "from=2026-12-01",
            "from=tomorrow&to=2026-12-31",
            "from=2026-12-01&to=2026-02-30",
            "from=2026-12-31&to=2026-12-01",
            "from=2026-01-01&to=2027-12-31&granularity=day",
            "from=2026-01-01&to=2026-12-31&granularity=week",
            "from=2026-01-01&to=2026-12-31&account=abc",
        ] {
            let reply = send(
                &app,
                get_request(&cash_flow_uri(company_id, query), Some(&cookie)),
            )
            .await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{query}");
            assert!(reply.body["error"].is_string(), "{query}");
        }
        let unknown = send(
            &app,
            get_request(
                &cash_flow_uri(
                    company_id,
                    &format!("from=2026-12-01&to=2026-12-31&account={}", Uuid::new_v4()),
                ),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(unknown.status, StatusCode::NOT_FOUND);

        // Another company's member does not learn that it exists, and without a session it is a 401.
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        let uri = cash_flow_uri(company_id, "from=2026-12-01&to=2026-12-31");
        assert_eq!(
            send(&app, get_request(&uri, Some(&stranger))).await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&uri, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn every_role_reads_the_cash_flow_and_none_can_write_it() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let uri = cash_flow_uri(company_id, "from=2026-12-01&to=2026-12-31");
        for role in [
            Role::Admin,
            Role::Financial,
            Role::Sales,
            Role::Stock,
            Role::Accountant,
            Role::Viewer,
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&uri, Some(&cookie))).await.status,
                StatusCode::OK,
                "{role:?} reads"
            );
        }
        let viewer = member_with_role(&app, &identities, company_id, Role::Viewer).await;
        let write = send(&app, json_request("POST", &uri, Some(&viewer), json!({}))).await;
        assert!(write.status.is_client_error(), "{:?}", write.status);
    }

    fn recurrences_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/recurrences")
    }

    fn recurrence_body(targets: &EntryTargets, description: &str) -> Value {
        // A span in the past that is bounded, so that what is generated does not depend on today.
        json!({
            "kind": "payable",
            "description": description,
            "category_id": targets.category,
            "amount": "120.50",
            "frequency": "monthly",
            "starts_on": "2026-01-05",
            "ends_on": "2026-03-05",
        })
    }

    #[tokio::test]
    async fn registers_a_recurrence_that_generates_its_entries_and_changes_only_what_comes_after() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let uri = recurrences_uri(company_id);

        let created = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                recurrence_body(&targets, "Aluguel"),
            ),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        assert_eq!(created.body["amount"], "120.5");
        assert_eq!(created.body["frequency"], "monthly");
        assert_eq!(created.body["active"], true);
        assert_eq!(created.body["generated_through"], "2026-03-05");
        let id = created.body["id"].as_str().unwrap().to_owned();

        // Its three occurrences are ordinary entries that know where they came from.
        let generated = send(
            &app,
            get_request(
                &format!("{}?q=Aluguel&sort=competence_date", entries_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        let items = generated.body["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        let days: Vec<&str> = items
            .iter()
            .map(|item| item["competence_date"].as_str().unwrap())
            .collect();
        assert_eq!(days, ["2026-01-05", "2026-02-05", "2026-03-05"]);
        assert!(items.iter().all(|item| item["recurrence_id"] == id
            && item["total"] == "120.5"
            && item["kind"] == "payable"));

        // Replaced as a whole: what was generated stays, and nothing new is made past its end.
        let one = format!("{uri}/{id}");
        let mut changed = recurrence_body(&targets, "Aluguel");
        changed["amount"] = json!("130");
        changed["active"] = json!(true);
        let replaced = send(
            &app,
            json_request("PUT", &one, Some(&cookie), changed.clone()),
        )
        .await;
        assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.body);
        assert_eq!(replaced.body["amount"], "130");
        let again = send(
            &app,
            json_request("PUT", &one, Some(&cookie), changed.clone()),
        )
        .await;
        assert_eq!(again.status, StatusCode::OK);
        let after = send(
            &app,
            get_request(
                &format!("{}?q=Aluguel", entries_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(after.body["items"].as_array().unwrap().len(), 3);
        assert!(
            after.body["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item["total"] == "120.5")
        );
        let history = send(&app, get_request(&format!("{one}/history"), Some(&cookie))).await;
        assert_eq!(
            history.body.as_array().unwrap().len(),
            2,
            "the same body again records nothing"
        );

        // Listed, narrowed and paged like the other registers.
        let second = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                recurrence_body(&targets, "Seguro"),
            ),
        )
        .await;
        assert_eq!(second.status, StatusCode::CREATED);
        let listed = send(&app, get_request(&uri, Some(&cookie))).await;
        let names: Vec<&str> = listed.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["description"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Aluguel", "Seguro"]);
        let page = send(&app, get_request(&format!("{uri}?limit=1"), Some(&cookie))).await;
        assert_eq!(page.body["items"].as_array().unwrap().len(), 1);
        let cursor = page.body["next_cursor"].as_str().unwrap();
        let rest = send(
            &app,
            get_request(&format!("{uri}?limit=1&after={cursor}"), Some(&cookie)),
        )
        .await;
        assert_eq!(rest.body["items"][0]["description"], "Seguro");
        let narrowed = send(
            &app,
            get_request(
                &format!("{uri}?q=seg&kind=payable&active=true&sort=-description"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(narrowed.body["items"].as_array().unwrap().len(), 1);
        for query in ["kind=sale", "active=maybe", "sort=name", "after=garbage"] {
            let reply = send(&app, get_request(&format!("{uri}?{query}"), Some(&cookie))).await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{query}");
            assert!(reply.body["error"].is_string(), "{query}");
        }

        // What is refused is JSON and makes no recurrence; a replacement has to say whether it is active.
        let bad = |field: &str, value: Value| {
            let mut body = recurrence_body(&targets, "X");
            body[field] = value;
            body
        };
        for body in [
            bad("amount", json!("0")),
            bad("amount", json!(10)),
            bad("frequency", json!("daily")),
            bad("kind", json!("sale")),
            bad("starts_on", json!("2026-02-30")),
            bad("ends_on", json!("2025-12-31")),
            bad("category_id", json!(Uuid::new_v4())),
        ] {
            let reply = send(&app, json_request("POST", &uri, Some(&cookie), body)).await;
            assert!(reply.status.is_client_error(), "{}", reply.body);
            assert!(reply.body["error"].is_string());
        }
        let missing_active = send(
            &app,
            json_request(
                "PUT",
                &one,
                Some(&cookie),
                recurrence_body(&targets, "Aluguel"),
            ),
        )
        .await;
        assert_eq!(
            missing_active.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            missing_active.body
        );
        assert_eq!(
            send(&app, get_request(&uri, Some(&cookie))).await.body["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        // Another company's member does not learn that it exists.
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        for request in [
            get_request(&uri, Some(&stranger)),
            json_request("PUT", &one, Some(&stranger), changed.clone()),
            get_request(&format!("{one}/history"), Some(&stranger)),
        ] {
            assert_eq!(send(&app, request).await.status, StatusCode::NOT_FOUND);
        }
        assert_eq!(
            send(
                &app,
                json_request("PUT", &format!("{uri}/not-an-id"), Some(&cookie), changed)
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&uri, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn recurrences_follow_the_finance_role() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let uri = recurrences_uri(company_id);
        for (role, may_write) in [
            (Role::Admin, true),
            (Role::Financial, true),
            (Role::Sales, false),
            (Role::Stock, false),
            (Role::Accountant, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&uri, Some(&cookie))).await.status,
                StatusCode::OK,
                "{role:?} reads"
            );
            let write = send(&app, json_request("POST", &uri, Some(&cookie), json!({}))).await;
            let expected = if may_write {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(write.status, expected, "{role:?} writes");
        }
    }

    /// A company's own gateway settings ([ADR 0085]): reached only by the owner and the admin (the `members` area: not even the role
    /// that writes the fiscal area), sealed by the server, and **no response, no trail and no error ever holds a secret**.
    #[tokio::test]
    async fn the_gateway_settings_are_for_the_owner_and_the_admin_and_never_show_a_secret() {
        use conta_verde_domain::Role;

        let (app, identities) = entry_app_with_identities();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let admin = member_with_role(&app, &identities, company_id, Role::Admin).await;
        let (stranger, _) = sign_up(&app, "outra@example.com").await;
        let uri = format!("/companies/{company_id}/gateway-settings");
        let secret = |action: &str, value: Option<&str>| match value {
            Some(value) => json!({ "action": action, "value": value }),
            None => json!({ "action": action }),
        };
        let body = |token: Value| {
            json!({
                "kind": "focus",
                "focus_token_homologation": token,
                "focus_token_production": secret("keep", None),
                "nfeio_api_key": secret("keep", None),
                "webhook_secret": secret("set", Some("hook_1234567890abcdef")),
                "nfeio_company_homologation": null,
                "nfeio_company_production": null,
                "nfeio_search_confirmed": false
            })
        };
        let typed = [
            "tok_homol_1234567890",
            "hook_1234567890abcdef",
            "tok_new_1234567890",
        ];
        let leaks = |text: &str| typed.iter().any(|value| text.contains(value));

        // Nothing set yet.
        let none = send(&app, get_request(&uri, Some(&owner))).await;
        assert_eq!(none.status, StatusCode::OK);
        assert_eq!(none.body["kind"], Value::Null);
        assert_eq!(none.body["encryption_available"], true);
        assert_eq!(
            none.body["webhook_path"],
            format!("/public/fiscal/{company_id}/webhook")
        );

        // Only the owner and the admin reach it: no other role reads it, none writes it, a stranger finds nothing.
        for role in [
            Role::Financial,
            Role::Sales,
            Role::Stock,
            Role::Accountant,
            Role::Viewer,
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&uri, Some(&cookie))).await.status,
                StatusCode::FORBIDDEN,
                "{role:?} reads"
            );
            assert_eq!(
                send(
                    &app,
                    json_request(
                        "PUT",
                        &uri,
                        Some(&cookie),
                        body(secret("set", Some("tok_homol_1234567890")))
                    )
                )
                .await
                .status,
                StatusCode::FORBIDDEN,
                "{role:?} writes"
            );
        }
        assert_eq!(
            send(&app, get_request(&uri, Some(&stranger))).await.status,
            StatusCode::NOT_FOUND
        );

        // Written, it answers with what is set and never a value.
        let saved = send(
            &app,
            json_request(
                "PUT",
                &uri,
                Some(&owner),
                body(secret("set", Some("tok_homol_1234567890"))),
            ),
        )
        .await;
        assert_eq!(saved.status, StatusCode::OK, "{}", saved.body);
        assert_eq!(saved.body["kind"], "focus");
        assert_eq!(saved.body["focus_token_homologation_set"], true);
        assert_eq!(saved.body["focus_token_production_set"], false);
        assert_eq!(saved.body["webhook_secret_set"], true);
        assert!(!leaks(&saved.body.to_string()));
        let read = send(&app, get_request(&uri, Some(&admin))).await;
        assert_eq!(read.status, StatusCode::OK);
        assert_eq!(read.body, saved.body);
        assert!(!leaks(&read.body.to_string()));

        // The admin changes it; the trail says a secret was set and changed, never what it was.
        let changed = send(
            &app,
            json_request(
                "PUT",
                &uri,
                Some(&admin),
                body(secret("set", Some("tok_new_1234567890"))),
            ),
        )
        .await;
        assert_eq!(changed.status, StatusCode::OK);
        let history = send(&app, get_request(&format!("{uri}/history"), Some(&owner))).await;
        assert_eq!(history.status, StatusCode::OK);
        assert_eq!(history.body.as_array().unwrap().len(), 2);
        assert!(!leaks(&history.body.to_string()), "{}", history.body);
        assert!(history.body.to_string().contains("changed"));

        // What is not a secret write, or is not a token, is refused with no value in the answer.
        for bad in [
            body(secret("set", Some("short"))),
            body(secret("set", None)),
            body(secret("keep", Some("tok_homol_1234567890"))),
            body(secret("erase", None)),
            {
                let mut b = body(secret("keep", None));
                b["kind"] = json!("fake");
                b
            },
            {
                let mut b = body(secret("keep", None));
                b["extra"] = json!(1);
                b
            },
            {
                let mut b = body(secret("keep", None));
                b["nfeio_company_production"] = json!("has space");
                b
            },
        ] {
            let refused = send(&app, json_request("PUT", &uri, Some(&owner), bad.clone())).await;
            assert!(
                matches!(
                    refused.status,
                    StatusCode::UNPROCESSABLE_ENTITY | StatusCode::BAD_REQUEST
                ),
                "{bad} -> {}",
                refused.status
            );
            assert!(!leaks(&refused.body.to_string()), "{}", refused.body);
        }
        assert_eq!(
            send(&app, get_request(&format!("{uri}/history"), Some(&owner)))
                .await
                .body
                .as_array()
                .unwrap()
                .len(),
            2
        );

        // Reset: only the owner and the admin, the server's settings come back, the trail says what was cleared and never a value.
        let reader = member_with_role(&app, &identities, company_id, Role::Financial).await;
        assert_eq!(
            send(
                &app,
                json_request("DELETE", &uri, Some(&reader), Value::Null)
            )
            .await
            .status,
            StatusCode::FORBIDDEN
        );
        let reset = send(
            &app,
            json_request("DELETE", &uri, Some(&admin), Value::Null),
        )
        .await;
        assert_eq!(reset.status, StatusCode::OK, "{}", reset.body);
        assert_eq!(reset.body["kind"], Value::Null);
        assert_eq!(reset.body["webhook_secret_set"], false);
        assert_eq!(reset.body["unreadable"], json!([]));
        let history = send(&app, get_request(&format!("{uri}/history"), Some(&owner))).await;
        assert_eq!(history.body.as_array().unwrap().len(), 3);
        assert!(!leaks(&history.body.to_string()));
    }

    /// The invoices issued against the company ([ADR 0083]): fetched from the gateway on request, declared to the authority in the
    /// name of the person with the rules of the words, each role keeping to its area; and the webhook only wakes the server up:
    /// a secret in the header the gateway was told to use, nothing in its body believed.
    #[tokio::test]
    async fn received_invoices_are_synced_declared_and_the_webhook_only_wakes_the_server() {
        use conta_verde_domain::Role;

        let (app, identities) = entry_app_with_identities();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let seller = member_with_role(&app, &identities, company_id, Role::Sales).await;
        let accountant = member_with_role(&app, &identities, company_id, Role::Accountant).await;
        let (stranger, _) = sign_up(&app, "outra@example.com").await;
        let received = format!("/companies/{company_id}/fiscal/received-invoices");
        let fake = gateway();

        // Nothing is asked before the company says how it issues.
        let early = send(
            &app,
            json_request(
                "POST",
                &format!("{received}/sync"),
                Some(&owner),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(early.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            send(
                &app,
                json_request(
                    "PUT",
                    &format!("/companies/{company_id}/fiscal/settings"),
                    Some(&owner),
                    fiscal_settings_body(),
                ),
            )
            .await
            .status,
            StatusCode::OK
        );
        for number in 1..=3 {
            fake.receive(received_invoice(number, "2026-11-02"));
        }

        // Only those who write the area fetch; everybody who reads the area reads what was fetched.
        for cookie in [&seller, &accountant] {
            assert_eq!(
                send(
                    &app,
                    json_request(
                        "POST",
                        &format!("{received}/sync"),
                        Some(cookie),
                        Value::Null
                    )
                )
                .await
                .status,
                StatusCode::FORBIDDEN
            );
        }
        let synced = send(
            &app,
            json_request(
                "POST",
                &format!("{received}/sync"),
                Some(&owner),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(synced.status, StatusCode::OK, "{}", synced.body);
        assert_eq!(synced.body["added"], 3);
        let listed = send(&app, get_request(&received, Some(&accountant))).await;
        assert_eq!(listed.status, StatusCode::OK);
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 3);
        assert_eq!(listed.body["items"][0]["manifestation"], "none");
        let id = listed.body["items"][0]["id"].as_str().unwrap().to_owned();
        let one = format!("{received}/{id}");
        assert_eq!(
            send(&app, get_request(&one, Some(&seller))).await.status,
            StatusCode::OK
        );
        assert_eq!(
            send(&app, get_request(&received, Some(&stranger)))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        let narrowed = send(
            &app,
            get_request(&format!("{received}?manifestation=awareness"), Some(&owner)),
        )
        .await;
        assert!(narrowed.body["items"].as_array().unwrap().is_empty());
        for bad in ["manifestation=nonsense", "after=garbage"] {
            assert_eq!(
                send(
                    &app,
                    get_request(&format!("{received}?{bad}"), Some(&owner))
                )
                .await
                .status,
                StatusCode::BAD_REQUEST,
                "{bad}"
            );
        }
        let paged = send(
            &app,
            get_request(&format!("{received}?limit=2"), Some(&owner)),
        )
        .await;
        assert_eq!(paged.body["items"].as_array().unwrap().len(), 2);
        let cursor = paged.body["next_cursor"].as_str().unwrap();
        let rest = send(
            &app,
            get_request(&format!("{received}?limit=2&after={cursor}"), Some(&owner)),
        )
        .await;
        assert_eq!(rest.body["items"].as_array().unwrap().len(), 1);

        // The words: the rules first, then the authority, then the trail in the name of the person.
        let manifest = format!("{one}/manifest");
        for (body, status) in [
            (json!({ "word": "none" }), StatusCode::UNPROCESSABLE_ENTITY),
            (
                json!({ "word": "somewhere" }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "word": "not_performed" }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "word": "awareness", "extra": 1 }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            assert_eq!(
                send(
                    &app,
                    json_request("POST", &manifest, Some(&owner), body.clone())
                )
                .await
                .status,
                status,
                "{body}"
            );
        }
        assert_eq!(
            send(
                &app,
                json_request(
                    "POST",
                    &manifest,
                    Some(&seller),
                    json!({ "word": "awareness" })
                )
            )
            .await
            .status,
            StatusCode::FORBIDDEN
        );
        let aware = send(
            &app,
            json_request(
                "POST",
                &manifest,
                Some(&owner),
                json!({ "word": "awareness" }),
            ),
        )
        .await;
        assert_eq!(aware.status, StatusCode::OK, "{}", aware.body);
        assert_eq!(aware.body["manifestation"], "awareness");
        let last = send(
            &app,
            json_request(
                "POST",
                &manifest,
                Some(&owner),
                json!({ "word": "not_performed", "justification": "A mercadoria nunca chegou ao estabelecimento" }),
            ),
        )
        .await;
        assert_eq!(last.body["manifestation"], "not_performed", "{}", last.body);
        assert_eq!(
            send(
                &app,
                json_request(
                    "POST",
                    &manifest,
                    Some(&owner),
                    json!({ "word": "confirmation" })
                )
            )
            .await
            .status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "nothing follows the last word"
        );
        let history = send(
            &app,
            get_request(&format!("{one}/history"), Some(&accountant)),
        )
        .await;
        assert_eq!(history.body.as_array().unwrap().len(), 2);
        assert_eq!(fake.manifested().len(), 2);
        assert_eq!(
            send(
                &app,
                get_request(&format!("{received}/{}", Uuid::new_v4()), Some(&owner))
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );

        // The webhook: no session, a secret in the header, nothing in the body believed, and it wakes the server up to ask.
        let hook = format!("/public/fiscal/{company_id}/webhook");
        let knock = |secret: Option<&str>, uri: &str| {
            let mut request = axum::http::Request::builder()
                .method("POST")
                .uri(uri.to_owned());
            if let Some(secret) = secret {
                request = request.header("x-fiscal-webhook-token", secret);
            }
            request
                .body(axum::body::Body::from(
                    r#"{"event":"nfe_recebida","chave":"nothing believed"}"#,
                ))
                .unwrap()
        };
        assert_eq!(
            send(&app, knock(None, &hook)).await.status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(&app, knock(Some("wrong"), &hook)).await.status,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            send(
                &app,
                knock(
                    Some(FISCAL_HOOK_SECRET),
                    &format!("/public/fiscal/{}/webhook", Uuid::new_v4())
                )
            )
            .await
            .status,
            StatusCode::NOT_FOUND,
            "a company that issues nothing"
        );
        assert_eq!(
            send(
                &app,
                knock(Some(FISCAL_HOOK_SECRET), "/public/fiscal/not-an-id/webhook")
            )
            .await
            .status,
            StatusCode::UNAUTHORIZED,
            "an id that is no company's is a knock that is not believed, like a wrong secret"
        );
        fake.receive(received_invoice(9, "2026-11-04"));
        assert_eq!(
            send(&app, knock(Some(FISCAL_HOOK_SECRET), &hook))
                .await
                .status,
            StatusCode::ACCEPTED
        );
        // The answer is at once and the work is done meanwhile: the knock made the server ask.
        let mut found = 0;
        for _ in 0..50 {
            let after = send(&app, get_request(&received, Some(&owner))).await;
            found = after.body["items"].as_array().unwrap().len();
            if found == 4 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(found, 4, "the knock made the server ask");

        // A company that set a webhook secret of its own is knocked at with that one **only**: the operator's no longer opens it,
        // and another company's does not either (ADR 0085).
        let own_secret = "own_hook_1234567890abcdef";
        let settings = send(
            &app,
            json_request(
                "PUT",
                &format!("/companies/{company_id}/gateway-settings"),
                Some(&owner),
                json!({
                    "kind": "focus",
                    "focus_token_homologation": { "action": "keep" },
                    "focus_token_production": { "action": "keep" },
                    "nfeio_api_key": { "action": "keep" },
                    "webhook_secret": { "action": "set", "value": own_secret },
                    "nfeio_company_homologation": null,
                    "nfeio_company_production": null,
                    "nfeio_search_confirmed": false
                }),
            ),
        )
        .await;
        assert_eq!(settings.status, StatusCode::OK, "{:?}", settings.body);
        assert_eq!(
            send(&app, knock(Some(FISCAL_HOOK_SECRET), &hook))
                .await
                .status,
            StatusCode::UNAUTHORIZED,
            "the operator's secret no longer opens a company that has its own"
        );
        assert_eq!(
            send(&app, knock(Some(own_secret), &hook)).await.status,
            StatusCode::ACCEPTED
        );
        let (_, other_company) = sign_up(&app, "vizinha@example.com").await;
        assert_eq!(
            send(
                &app,
                knock(
                    Some(own_secret),
                    &format!("/public/fiscal/{other_company}/webhook")
                )
            )
            .await
            .status,
            StatusCode::UNAUTHORIZED,
            "a company's secret opens no other company"
        );
        assert_eq!(
            send(
                &app,
                knock(
                    Some(own_secret),
                    &format!("/public/fiscal/{}/webhook", Uuid::new_v4())
                )
            )
            .await
            .status,
            StatusCode::UNAUTHORIZED,
            "and a company that does not exist answers like a wrong secret"
        );
    }

    /// An NFC-e through the API ([ADR 0082]): the product lines of a completed sale at the counter, what is paid adding up to the
    /// total, the consumer named only when the person says so, no correction letter, and each role keeping to its area.
    #[tokio::test]
    async fn an_nfce_is_issued_at_the_counter_from_the_product_lines_of_a_sale() {
        use conta_verde_domain::Role;

        let (app, identities) = entry_app_with_identities();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let seller = member_with_role(&app, &identities, company_id, Role::Sales).await;
        let targets = sales_targets(&app, &owner, company_id).await;
        let category = revenue_category(&app, &owner, company_id).await;
        let customer = send(
            &app,
            json_request(
                "POST",
                &parties_uri(company_id),
                Some(&owner),
                json!({ "name": "Maria Souza", "document": "529.982.247-25", "is_customer": true, "is_supplier": false }),
            ),
        )
        .await;
        let mut sale = sale_body(&targets, &category);
        sale["customer_id"] = customer.body["id"].clone();
        let created = send(
            &app,
            json_request("POST", &sales_uri(company_id), Some(&owner), sale),
        )
        .await;
        let sale_id = created.body["id"].as_str().unwrap().to_owned();
        assert_eq!(
            send(
                &app,
                json_request(
                    "POST",
                    &format!("{}/{sale_id}/complete", sales_uri(company_id)),
                    Some(&owner),
                    Value::Null,
                ),
            )
            .await
            .status,
            StatusCode::OK
        );
        assert_eq!(
            send(
                &app,
                json_request(
                    "PUT",
                    &format!("/companies/{company_id}/fiscal/settings"),
                    Some(&owner),
                    fiscal_settings_body(),
                ),
            )
            .await
            .status,
            StatusCode::OK
        );

        let consumers = format!("/companies/{company_id}/fiscal/consumer-documents");
        let choices = json!({
            "sale_id": sale_id, "nature_code": "sale", "origin": 0, "presence": "in_person",
            "identify_consumer": true, "payments": [{ "form": "pix", "amount": "35" }]
        });
        assert_eq!(
            send(
                &app,
                json_request("POST", &consumers, Some(&seller), choices.clone())
            )
            .await
            .status,
            StatusCode::FORBIDDEN
        );
        for (field, value) in [
            ("presence", json!("somewhere")),
            ("payments", json!([{ "form": "gold", "amount": "35" }])),
            ("payments", json!([{ "form": "pix", "amount": "abc" }])),
            ("extra", json!(1)),
        ] {
            let mut bad = choices.clone();
            bad[field] = value;
            assert_eq!(
                send(&app, json_request("POST", &consumers, Some(&owner), bad))
                    .await
                    .status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{field}"
            );
        }
        // What is paid must add up: no change is given.
        let mut short = choices.clone();
        short["payments"] = json!([{ "form": "cash", "amount": "20" }]);
        let refused = send(&app, json_request("POST", &consumers, Some(&owner), short)).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(refused.body["error"].as_str().unwrap().contains("add up"));

        let issued = send(
            &app,
            json_request("POST", &consumers, Some(&owner), choices.clone()),
        )
        .await;
        assert_eq!(issued.status, StatusCode::OK, "{}", issued.body);
        assert_eq!(issued.body["kind"], "nfce");
        assert_eq!(issued.body["total"], "35");
        assert_eq!(issued.body["recipient_document"], "52998224725");
        let again = send(
            &app,
            json_request("POST", &consumers, Some(&owner), choices.clone()),
        )
        .await;
        assert_eq!(again.body["id"], issued.body["id"]);
        assert_eq!(again.body["status"], "authorized", "{}", again.body);
        assert_eq!(again.body["access_key"].as_str().unwrap().len(), 44);
        assert_eq!(&again.body["access_key"].as_str().unwrap()[20..22], "65");
        let one = format!(
            "/companies/{company_id}/fiscal/documents/{}",
            issued.body["id"].as_str().unwrap()
        );
        let letter = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/corrections"),
                Some(&owner),
                json!({ "text": "Corrige o endereço do consumidor" }),
            ),
        )
        .await;
        assert_eq!(letter.status, StatusCode::UNPROCESSABLE_ENTITY);
        let cancelled = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/cancel"),
                Some(&owner),
                json!({ "reason": "Venda desfeita a pedido do cliente" }),
            ),
        )
        .await;
        assert_eq!(cancelled.body["status"], "cancelled", "{}", cancelled.body);
    }

    /// An NFS-e through the API ([ADR 0081]): the service lines of a completed sale are invoiced once, under the nature's
    /// ISSQN terms, with a DPS number the company gave and never reuses; the NF-e of the same sale is the products' alone;
    /// the national NFS-e takes no correction letter; each role keeps to its area.
    #[tokio::test]
    async fn an_nfse_is_issued_from_the_service_lines_of_a_sale_and_cancelled() {
        use conta_verde_domain::Role;

        let (app, identities) = entry_app_with_identities();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let seller = member_with_role(&app, &identities, company_id, Role::Sales).await;
        let accountant = member_with_role(&app, &identities, company_id, Role::Accountant).await;
        let targets = sales_targets(&app, &owner, company_id).await;
        let category = revenue_category(&app, &owner, company_id).await;
        let service = send(
            &app,
            json_request(
                "POST",
                &products_uri(company_id),
                Some(&owner),
                json!({
                    "kind": "service", "name": "Consultoria", "sku": "consult", "unit": "h",
                    "ncm": null, "sale_price": "100", "cost_price": "0", "minimum_stock": null,
                    "active": true
                }),
            ),
        )
        .await;
        assert_eq!(service.status, StatusCode::CREATED, "{}", service.body);

        let customer = send(
            &app,
            json_request(
                "POST",
                &parties_uri(company_id),
                Some(&owner),
                json!({
                    "name": "Maria Souza", "document": "529.982.247-25",
                    "is_customer": true, "is_supplier": false,
                    "address": {
                        "postal_code": "01310-100", "street": "Rua Augusta", "number": "10",
                        "complement": null, "district": "Consolação", "city": "São Paulo", "state": "SP"
                    }
                }),
            ),
        )
        .await;
        let mut sale = sale_body(&targets, &category);
        sale["customer_id"] = customer.body["id"].clone();
        sale["lines"] = json!([
            { "product_id": targets.product, "description": "Café 500g", "quantity": "3",
              "unit_price": "10.10", "discount": "0.30" },
            { "product_id": service.body["id"], "description": "Consultoria", "quantity": "2",
              "unit_price": "100.00", "discount": "10" },
        ]);
        let created = send(
            &app,
            json_request("POST", &sales_uri(company_id), Some(&owner), sale),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        let sale_id = created.body["id"].as_str().unwrap().to_owned();
        let done = send(
            &app,
            json_request(
                "POST",
                &format!("{}/{sale_id}/complete", sales_uri(company_id)),
                Some(&owner),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(done.status, StatusCode::OK, "{}", done.body);

        let services = format!("/companies/{company_id}/fiscal/service-documents");
        let choices = json!({
            "sale_id": sale_id, "nature_code": "service", "competence": "2026-11-09",
            "service_city_code": "3550308", "special_regime": "none"
        });
        assert_eq!(
            send(
                &app,
                json_request(
                    "PUT",
                    &format!("/companies/{company_id}/fiscal/settings"),
                    Some(&owner),
                    fiscal_settings_body(),
                ),
            )
            .await
            .status,
            StatusCode::OK
        );

        // Only those who write the area issue, and every choice is the person's and has a shape.
        for cookie in [&seller, &accountant] {
            assert_eq!(
                send(
                    &app,
                    json_request("POST", &services, Some(cookie), choices.clone())
                )
                .await
                .status,
                StatusCode::FORBIDDEN
            );
        }
        for (field, value) in [
            ("special_regime", json!("somewhere")),
            ("competence", json!("09/11/2026")),
            ("extra", json!(1)),
        ] {
            let mut bad = choices.clone();
            bad[field] = value;
            assert_eq!(
                send(&app, json_request("POST", &services, Some(&owner), bad))
                    .await
                    .status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{field}"
            );
        }
        let mut product_nature = choices.clone();
        product_nature["nature_code"] = json!("sale");
        let refused = send(
            &app,
            json_request("POST", &services, Some(&owner), product_nature),
        )
        .await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(refused.body["error"].as_str().unwrap().contains("services"));

        // Issued: numbered and waiting; asking again is the same document, authorized with an NFS-e's identity.
        let issued = send(
            &app,
            json_request("POST", &services, Some(&owner), choices.clone()),
        )
        .await;
        assert_eq!(issued.status, StatusCode::OK, "{}", issued.body);
        assert_eq!(issued.body["kind"], "nfse");
        assert_eq!(issued.body["status"], "processing");
        assert_eq!(issued.body["total"], "190");
        assert_eq!(issued.body["dps"], json!({ "series": 1, "number": 1 }));
        let again = send(
            &app,
            json_request("POST", &services, Some(&owner), choices.clone()),
        )
        .await;
        assert_eq!(again.body["id"], issued.body["id"]);
        assert_eq!(again.body["status"], "authorized", "{}", again.body);
        assert_eq!(again.body["access_key"], Value::Null);
        assert_eq!(again.body["nfse"]["number"], 1);
        assert!(again.body["nfse"]["verification_code"].is_string());
        for file in ["xml", "pdf"] {
            assert!(
                again.body["files"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(file))
            );
        }
        let one = format!(
            "/companies/{company_id}/fiscal/documents/{}",
            issued.body["id"].as_str().unwrap()
        );
        let (status, headers, bytes) = raw(
            &app,
            get_request(&format!("{one}/files/xml"), Some(&accountant)),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(!bytes.is_empty());

        // The NF-e of the same sale is the products' alone and lives beside the NFS-e.
        let nfe = send(
            &app,
            json_request(
                "POST",
                &format!("/companies/{company_id}/fiscal/documents"),
                Some(&owner),
                json!({
                    "sale_id": sale_id, "nature_code": "sale", "origin": 0, "presence": "internet",
                    "payment": "pix", "final_consumer": true
                }),
            ),
        )
        .await;
        assert_eq!(nfe.status, StatusCode::OK, "{}", nfe.body);
        assert_eq!(nfe.body["kind"], "nfe");
        assert_eq!(nfe.body["total"], "30");
        assert_ne!(nfe.body["id"], issued.body["id"]);

        // The national NFS-e takes no correction letter; it is cancelled with a reason, and then the sale is free for a
        // new one, which takes the next DPS number.
        let letter = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/corrections"),
                Some(&owner),
                json!({ "text": "Corrige a descrição do serviço prestado" }),
            ),
        )
        .await;
        assert_eq!(letter.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            letter.body["error"]
                .as_str()
                .unwrap()
                .contains("no correction letter")
        );
        let cancelled = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/cancel"),
                Some(&owner),
                json!({ "reason": "Serviço não prestado, nota emitida por engano" }),
            ),
        )
        .await;
        assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.body);
        assert_eq!(cancelled.body["status"], "cancelled");
        let second = send(
            &app,
            json_request("POST", &services, Some(&owner), choices.clone()),
        )
        .await;
        assert_ne!(second.body["id"], issued.body["id"]);
        assert_eq!(second.body["dps"], json!({ "series": 1, "number": 2 }));

        // The list tells the kind of each document.
        let listed = send(
            &app,
            get_request(
                &format!("/companies/{company_id}/fiscal/documents"),
                Some(&seller),
            ),
        )
        .await;
        let kinds: Vec<&str> = listed.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds.iter().filter(|kind| **kind == "nfse").count(), 2);
        assert_eq!(kinds.iter().filter(|kind| **kind == "nfe").count(), 1);
    }

    /// An NF-e through the API ([ADR 0076]): a completed sale to a customer in the issuer's state is invoiced once
    /// however many times it is asked, the gateway authorizes it after a question, the XML is a download that is never
    /// shown as a page, a cancellation keeps the file and frees the sale, and each role keeps to its area.
    #[tokio::test]
    async fn an_nfe_is_issued_from_a_sale_followed_and_cancelled() {
        use conta_verde_domain::Role;

        let (app, identities) = entry_app_with_identities();
        let (owner, company_id) = sign_up(&app, "dona@example.com").await;
        let seller = member_with_role(&app, &identities, company_id, Role::Sales).await;
        let accountant = member_with_role(&app, &identities, company_id, Role::Accountant).await;
        let (stranger, _) = sign_up(&app, "outra@example.com").await;
        let targets = sales_targets(&app, &owner, company_id).await;
        let category = revenue_category(&app, &owner, company_id).await;

        // A customer in the issuer's state with a CPF, and a completed sale to them.
        let customer = send(
            &app,
            json_request(
                "POST",
                &parties_uri(company_id),
                Some(&owner),
                json!({
                    "name": "Maria Souza",
                    "document": "529.982.247-25",
                    "is_customer": true,
                    "is_supplier": false,
                    "address": {
                        "postal_code": "01310-100", "street": "Rua Augusta", "number": "10",
                        "complement": null, "district": "Consolação", "city": "São Paulo", "state": "SP"
                    }
                }),
            ),
        )
        .await;
        assert_eq!(customer.status, StatusCode::CREATED, "{}", customer.body);
        let mut sale = sale_body(&targets, &category);
        sale["customer_id"] = customer.body["id"].clone();
        let created = send(
            &app,
            json_request("POST", &sales_uri(company_id), Some(&owner), sale),
        )
        .await;
        let sale_id = created.body["id"].as_str().unwrap().to_owned();
        let done = send(
            &app,
            json_request(
                "POST",
                &format!("{}/{sale_id}/complete", sales_uri(company_id)),
                Some(&owner),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(done.status, StatusCode::OK, "{}", done.body);

        let documents = format!("/companies/{company_id}/fiscal/documents");
        let choices = json!({
            "sale_id": sale_id, "nature_code": "sale", "origin": 0, "presence": "internet",
            "payment": "pix", "final_consumer": true
        });

        // Nothing is issued before the company says how it issues; and only those who write the area issue.
        let early = send(
            &app,
            json_request("POST", &documents, Some(&owner), choices.clone()),
        )
        .await;
        assert_eq!(
            early.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            early.body
        );
        assert!(
            early.body["error"]
                .as_str()
                .unwrap()
                .contains("fiscal settings")
        );
        assert_eq!(
            send(
                &app,
                json_request(
                    "PUT",
                    &format!("/companies/{company_id}/fiscal/settings"),
                    Some(&owner),
                    fiscal_settings_body(),
                ),
            )
            .await
            .status,
            StatusCode::OK
        );
        for cookie in [&seller, &accountant] {
            let denied = send(
                &app,
                json_request("POST", &documents, Some(cookie), choices.clone()),
            )
            .await;
            assert_eq!(denied.status, StatusCode::FORBIDDEN);
        }
        let mut bad = choices.clone();
        bad["presence"] = json!("somewhere");
        assert_eq!(
            send(&app, json_request("POST", &documents, Some(&owner), bad))
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let mut unknown = choices.clone();
        unknown["extra"] = json!(1);
        assert_eq!(
            send(
                &app,
                json_request("POST", &documents, Some(&owner), unknown)
            )
            .await
            .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );

        // Issued: waiting for the authority; asking again is the same document, now authorized with its files.
        let issued = send(
            &app,
            json_request("POST", &documents, Some(&owner), choices.clone()),
        )
        .await;
        assert_eq!(issued.status, StatusCode::OK, "{}", issued.body);
        assert_eq!(issued.body["status"], "processing");
        assert_eq!(issued.body["total"], "35");
        let id = issued.body["id"].as_str().unwrap().to_owned();
        let one = format!("{documents}/{id}");
        // The list: the company's documents, newest first, narrowed by status and sale, never another company's.
        let listed = send(&app, get_request(&documents, Some(&seller))).await;
        assert_eq!(listed.status, StatusCode::OK, "{}", listed.body);
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(listed.body["items"][0]["id"], issued.body["id"]);
        assert_eq!(listed.body["next_cursor"], Value::Null);
        for (query, count) in [
            ("status=processing", 1),
            ("status=cancelled", 0),
            (format!("sale={sale_id}").as_str(), 1),
            (format!("sale={}", Uuid::new_v4()).as_str(), 0),
        ] {
            let found = send(
                &app,
                get_request(&format!("{documents}?{query}"), Some(&owner)),
            )
            .await;
            assert_eq!(
                found.body["items"].as_array().unwrap().len(),
                count,
                "{query}"
            );
        }
        for bad in ["status=nonsense", "after=garbage"] {
            assert_eq!(
                send(
                    &app,
                    get_request(&format!("{documents}?{bad}"), Some(&owner))
                )
                .await
                .status,
                StatusCode::BAD_REQUEST,
                "{bad}"
            );
        }
        assert_eq!(
            send(&app, get_request(&documents, Some(&stranger)))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        let again = send(
            &app,
            json_request("POST", &documents, Some(&owner), choices.clone()),
        )
        .await;
        assert_eq!(again.body["id"], issued.body["id"]);
        assert_eq!(again.body["status"], "authorized", "{}", again.body);
        assert_eq!(again.body["access_key"].as_str().unwrap().len(), 44);
        assert!(
            again.body["files"]
                .as_array()
                .unwrap()
                .contains(&json!("xml"))
        );

        // The accountant reads it and downloads the XML, which is never a page of the app's own origin.
        let read = send(&app, get_request(&one, Some(&accountant))).await;
        assert_eq!(read.status, StatusCode::OK);
        let (status, headers, bytes) = raw(
            &app,
            get_request(&format!("{one}/files/xml"), Some(&accountant)),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "application/xml");
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(
            headers[header::CONTENT_DISPOSITION]
                .to_str()
                .unwrap()
                .starts_with("attachment")
        );
        assert!(!bytes.is_empty());
        let history = send(&app, get_request(&format!("{one}/history"), Some(&owner))).await;
        assert_eq!(history.body.as_array().unwrap().len(), 3);

        // Another company's person finds nothing.
        for uri in [
            one.clone(),
            format!("{one}/files/xml"),
            format!("{one}/history"),
        ] {
            assert_eq!(
                send(&app, get_request(&uri, Some(&stranger))).await.status,
                StatusCode::NOT_FOUND,
                "{uri}"
            );
        }

        // A correction letter is recorded and answered, its XML is a download; an invalidation never takes a number in use.
        let letters = format!("{one}/corrections");
        let short = send(
            &app,
            json_request("POST", &letters, Some(&owner), json!({ "text": "curta" })),
        )
        .await;
        assert_eq!(short.status, StatusCode::UNPROCESSABLE_ENTITY);
        let letter = send(
            &app,
            json_request(
                "POST",
                &letters,
                Some(&owner),
                json!({ "text": "Corrige o endereço de entrega do destinatário" }),
            ),
        )
        .await;
        assert_eq!(letter.status, StatusCode::OK, "{}", letter.body);
        assert_eq!(letter.body["status"], "accepted");
        assert_eq!(letter.body["sequence"], 1);
        assert_eq!(letter.body["has_file"], true);
        let (status, headers, bytes) = raw(
            &app,
            get_request(
                &format!(
                    "/companies/{company_id}/fiscal/events/{}/file",
                    letter.body["id"].as_str().unwrap()
                ),
                Some(&accountant),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(!bytes.is_empty());
        let listed = send(&app, get_request(&letters, Some(&seller))).await;
        assert_eq!(listed.body.as_array().unwrap().len(), 1);
        assert_eq!(
            send(
                &app,
                json_request(
                    "POST",
                    &letters,
                    Some(&seller),
                    json!({ "text": "Corrige o endereço de entrega" })
                ),
            )
            .await
            .status,
            StatusCode::FORBIDDEN
        );
        let invalidations = format!("/companies/{company_id}/fiscal/invalidations");
        let used = send(
            &app,
            json_request(
                "POST",
                &invalidations,
                Some(&owner),
                json!({ "series": 1, "first": 1, "last": 1_000_000, "reason": "Numeração pulada por falha do sistema" }),
            ),
        )
        .await;
        assert_eq!(
            used.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            used.body
        );
        let free = send(
            &app,
            json_request(
                "POST",
                &invalidations,
                Some(&owner),
                json!({ "series": 1, "first": 9000, "last": 9001, "reason": "Numeração pulada por falha do sistema" }),
            ),
        )
        .await;
        assert_eq!(free.status, StatusCode::OK, "{}", free.body);
        assert_eq!(free.body["first"], 9000);
        let all = send(&app, get_request(&invalidations, Some(&accountant))).await;
        assert_eq!(all.body.as_array().unwrap().len(), 1);

        // A cancellation needs a real reason; it keeps the files and frees the sale for a new document.
        assert_eq!(
            send(
                &app,
                json_request(
                    "POST",
                    &format!("{one}/cancel"),
                    Some(&owner),
                    json!({ "reason": "curto" })
                ),
            )
            .await
            .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let cancelled = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/cancel"),
                Some(&owner),
                json!({ "reason": "Pedido feito em duplicidade" }),
            ),
        )
        .await;
        assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.body);
        assert_eq!(cancelled.body["status"], "cancelled");
        assert!(
            cancelled.body["files"]
                .as_array()
                .unwrap()
                .contains(&json!("cancellation_xml"))
        );
        let second = send(
            &app,
            json_request("POST", &documents, Some(&owner), choices),
        )
        .await;
        assert_ne!(second.body["id"], issued.body["id"]);
    }

    fn quotes_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/quotes")
    }

    /// What a quote points at, made through the API: a customer, two products and a payment term.
    struct SalesTargets {
        customer: String,
        product: String,
        other_product: String,
        term: String,
    }

    async fn sales_targets(app: &Router, cookie: &str, company_id: Uuid) -> SalesTargets {
        let id_of = |reply: Reply| {
            assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
            reply.body["id"].as_str().unwrap().to_owned()
        };
        let product = |name: &str, sku: &str| {
            let mut body = product_body();
            body["name"] = json!(name);
            body["sku"] = json!(sku);
            post_product(app, cookie, company_id, body)
        };
        SalesTargets {
            customer: id_of(
                send(
                    app,
                    json_request("POST", &parties_uri(company_id), Some(cookie), party_body()),
                )
                .await,
            ),
            product: id_of(product("Café", "cafe").await),
            other_product: id_of(product("Pão", "pao").await),
            term: id_of(
                send(
                    app,
                    json_request(
                        "POST",
                        &payment_terms_uri(company_id),
                        Some(cookie),
                        json!({
                            "name": "30/60",
                            "installments": [
                                { "days": 30, "percent": "50" },
                                { "days": 60, "percent": "50" }
                            ]
                        }),
                    ),
                )
                .await,
            ),
        }
    }

    fn quote_body(targets: &SalesTargets, valid_until: &str) -> Value {
        json!({
            "customer_id": targets.customer,
            "issued_on": "2026-11-06",
            "valid_until": valid_until,
            "payment_term_id": targets.term,
            "notes": "Entrega em 5 dias",
            "lines": [
                { "product_id": targets.product, "description": "Café 500g", "quantity": "3",
                  "unit_price": "10.10", "discount": "0.30" },
                { "product_id": targets.other_product, "description": "Pão francês", "quantity": "1.5",
                  "unit_price": "3.33" }
            ],
        })
    }

    async fn raw(
        app: &Router,
        request: Request<Body>,
    ) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
        let response = app.clone().oneshot(request).await.unwrap();
        let (parts, body) = response.into_parts();
        (
            parts.status,
            parts.headers,
            body.collect().await.unwrap().to_bytes().to_vec(),
        )
    }

    #[tokio::test]
    async fn writes_changes_sends_and_gets_a_quote_answered_through_its_link() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let uri = quotes_uri(company_id);

        let created = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                quote_body(&targets, "2200-12-31"),
            ),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        assert_eq!(created.body["number"], 1);
        assert_eq!(created.body["status"], "draft");
        // 3 x 10.10 less 0.30, and 1.5 x 3.33 = 4.995 rounded to 5.
        assert_eq!(created.body["total"], "35");
        assert_eq!(created.body["lines"][0]["total"], "30");
        assert_eq!(created.body["lines"][1]["discount"], "0");
        let id = created.body["id"].as_str().unwrap().to_owned();
        let one = format!("{uri}/{id}");
        assert_eq!(
            send(&app, get_request(&one, Some(&cookie))).await.body,
            created.body
        );

        // Refused: what is wrong is said, and nothing is written.
        for (field, value, status) in [
            ("lines", json!([]), StatusCode::UNPROCESSABLE_ENTITY),
            (
                "customer_id",
                json!(Uuid::new_v4()),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                "valid_until",
                json!("2026-11-05"),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            let mut body = quote_body(&targets, "2200-12-31");
            body[field] = value;
            let reply = send(&app, json_request("POST", &uri, Some(&cookie), body)).await;
            assert_eq!(reply.status, status, "{field}");
        }
        let mut body = quote_body(&targets, "2200-12-31");
        body["lines"][0]["quantity"] = json!(3);
        assert_eq!(
            send(&app, json_request("POST", &uri, Some(&cookie), body))
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let listed = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);

        // Changed, with a history; listed by status and by text.
        let mut changed = quote_body(&targets, "2200-12-31");
        changed["lines"][0]["quantity"] = json!("4");
        let replaced = send(&app, json_request("PUT", &one, Some(&cookie), changed)).await;
        assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.body);
        assert_eq!(replaced.body["total"], "45.1");
        let history = send(&app, get_request(&format!("{one}/history"), Some(&cookie))).await;
        assert_eq!(history.body.as_array().unwrap().len(), 2);
        let found = send(
            &app,
            get_request(&format!("{uri}?q=franc%C3%AAs&status=draft"), Some(&cookie)),
        )
        .await;
        assert_eq!(found.body["items"].as_array().unwrap().len(), 1);
        let none = send(
            &app,
            get_request(&format!("{uri}?status=sent"), Some(&cookie)),
        )
        .await;
        assert_eq!(none.body["items"].as_array().unwrap().len(), 0);
        let bad = send(
            &app,
            get_request(&format!("{uri}?status=open"), Some(&cookie)),
        )
        .await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
        let bad = send(
            &app,
            get_request(&format!("{uri}?sort=name"), Some(&cookie)),
        )
        .await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);

        // Sent: the link shows the quote with no session, in PDF as well.
        let bad_address = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/send"),
                Some(&cookie),
                json!({ "to": "x" }),
            ),
        )
        .await;
        assert_eq!(bad_address.status, StatusCode::UNPROCESSABLE_ENTITY);
        let sent = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/send"),
                Some(&cookie),
                json!({ "to": "cliente@exemplo.com" }),
            ),
        )
        .await;
        assert_eq!(sent.status, StatusCode::OK, "{}", sent.body);
        assert_eq!(sent.body["quote"]["status"], "sent");
        assert_eq!(sent.body["emailed"], true);
        let link = sent.body["link"].as_str().unwrap();
        let token = link.rsplit('/').next().unwrap();
        let public = format!("/public/quotes/{token}");
        let seen = send(&app, get_request(&public, None)).await;
        assert_eq!(seen.status, StatusCode::OK, "{}", seen.body);
        assert_eq!(seen.body["company_name"], "Padaria");
        assert_eq!(seen.body["customer_name"], "Padaria Central");
        assert_eq!(seen.body["payment_term"], "30/60");
        assert_eq!(seen.body["number"], 1);
        assert_eq!(seen.body["expired"], false);
        assert_eq!(seen.body["total"], "45.1");
        assert_eq!(seen.body["lines"][0]["description"], "Café 500g");
        // The public page shows no id of the company's books, not even a product's.
        assert!(seen.body.get("company_id").is_none() && seen.body.get("id").is_none());
        assert!(seen.body["lines"][0].get("product_id").is_none());
        for uri in [format!("{public}/pdf"), format!("{one}/pdf")] {
            let request = if uri.starts_with("/public") {
                get_request(&uri, None)
            } else {
                get_request(&uri, Some(&cookie))
            };
            let (status, headers, bytes) = raw(&app, request).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
            assert_eq!(headers[header::CONTENT_TYPE], "application/pdf");
            assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
            if uri.starts_with("/public") {
                // A customer's data under a secret in the path: nothing keeps it, nothing passes it on.
                assert_eq!(headers[header::CACHE_CONTROL], "no-store");
                assert_eq!(headers[header::REFERRER_POLICY], "no-referrer");
            }
            assert!(bytes.starts_with(b"%PDF-1.4"));
            assert!(bytes.windows(9).any(|window| window == b"Or\xE7amento"));
        }
        assert_eq!(
            send(&app, get_request("/public/quotes/not-a-token", None))
                .await
                .status,
            StatusCode::NOT_FOUND
        );

        // Answered once; then it is fixed.
        let approved = send(
            &app,
            json_request("POST", &format!("{public}/approve"), None, Value::Null),
        )
        .await;
        assert_eq!(approved.status, StatusCode::OK, "{}", approved.body);
        assert_eq!(approved.body["status"], "approved");
        for action in ["approve", "decline"] {
            let again = send(
                &app,
                json_request("POST", &format!("{public}/{action}"), None, Value::Null),
            )
            .await;
            assert_eq!(again.status, StatusCode::CONFLICT, "{action}");
        }
        let locked = send(
            &app,
            json_request(
                "PUT",
                &one,
                Some(&cookie),
                quote_body(&targets, "2200-12-31"),
            ),
        )
        .await;
        assert_eq!(locked.status, StatusCode::CONFLICT);
        let history = send(&app, get_request(&format!("{one}/history"), Some(&cookie))).await;
        let last = history.body.as_array().unwrap().last().unwrap().clone();
        assert_eq!(last["actor"]["name"], "Ana");

        // One past its validity cannot be sent, since nobody could approve it.
        let mut old = quote_body(&targets, "2020-02-01");
        old["issued_on"] = json!("2020-01-01");
        let old = send(&app, json_request("POST", &uri, Some(&cookie), old)).await;
        let old_id = old.body["id"].as_str().unwrap();
        let late = send(
            &app,
            json_request(
                "POST",
                &format!("{uri}/{old_id}/send"),
                Some(&cookie),
                json!({ "to": "cliente@exemplo.com" }),
            ),
        )
        .await;
        assert_eq!(late.status, StatusCode::GONE);
    }

    #[tokio::test]
    async fn another_company_sees_nothing_of_a_quote() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let (other_cookie, other_company) = sign_up(&app, "bia@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let created = send(
            &app,
            json_request(
                "POST",
                &quotes_uri(company_id),
                Some(&cookie),
                quote_body(&targets, "2200-12-31"),
            ),
        )
        .await;
        let id = created.body["id"].as_str().unwrap();
        for path in ["", "/history", "/pdf"] {
            let reply = send(
                &app,
                get_request(
                    &format!("{}/{id}{path}", quotes_uri(other_company)),
                    Some(&other_cookie),
                ),
            )
            .await;
            assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        }
        let listed = send(
            &app,
            get_request(&quotes_uri(other_company), Some(&other_cookie)),
        )
        .await;
        assert!(listed.body["items"].as_array().unwrap().is_empty());
        assert_eq!(
            send(
                &app,
                get_request(&quotes_uri(company_id), Some(&other_cookie))
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&quotes_uri(company_id), None))
                .await
                .status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn quotes_follow_the_sales_role() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let uri = quotes_uri(company_id);
        for (role, may_write) in [
            (Role::Admin, true),
            (Role::Sales, true),
            (Role::Financial, false),
            (Role::Stock, false),
            (Role::Accountant, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&uri, Some(&cookie))).await.status,
                StatusCode::OK,
                "{role:?} reads"
            );
            let write = send(&app, json_request("POST", &uri, Some(&cookie), json!({}))).await;
            let expected = if may_write {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(write.status, expected, "{role:?} writes");
        }
    }

    fn sales_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/sales")
    }

    async fn revenue_category(app: &Router, cookie: &str, company_id: Uuid) -> String {
        let reply = post_category(
            app,
            cookie,
            company_id,
            json!({ "name": "Vendas", "dre_group": "gross_revenue", "cash_flow_group": "operating" }),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
        reply.body["id"].as_str().unwrap().to_owned()
    }

    fn sale_body(targets: &SalesTargets, category: &str) -> Value {
        json!({
            "customer_id": targets.customer,
            "sold_on": "2026-11-06",
            "payment_term_id": targets.term,
            "category_id": category,
            "notes": "Entrega em 5 dias",
            "lines": [
                { "product_id": targets.product, "description": "Café 500g", "quantity": "3",
                  "unit_price": "10.10", "discount": "0.30" },
                { "product_id": targets.other_product, "description": "Pão francês", "quantity": "1.5",
                  "unit_price": "3.33" }
            ],
        })
    }

    #[tokio::test]
    async fn a_quote_becomes_a_sale_whose_receivables_appear_in_the_cash_flow() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let category = revenue_category(&app, &cookie, company_id).await;

        // The customer approves the quote through its link.
        let quote = send(
            &app,
            json_request(
                "POST",
                &quotes_uri(company_id),
                Some(&cookie),
                quote_body(&targets, "2200-12-31"),
            ),
        )
        .await;
        let quote_id = quote.body["id"].as_str().unwrap().to_owned();
        let sent = send(
            &app,
            json_request(
                "POST",
                &format!("{}/{quote_id}/send", quotes_uri(company_id)),
                Some(&cookie),
                json!({ "to": "cliente@exemplo.com" }),
            ),
        )
        .await;
        let token = sent.body["link"]
            .as_str()
            .unwrap()
            .rsplit('/')
            .next()
            .unwrap()
            .to_owned();
        let approved = send(
            &app,
            json_request(
                "POST",
                &format!("/public/quotes/{token}/approve"),
                None,
                Value::Null,
            ),
        )
        .await;
        assert_eq!(approved.body["status"], "approved");

        // It becomes an open order: the quote's lines and term, a category for the receivable.
        let convert_uri = format!("{}/{quote_id}/convert", quotes_uri(company_id));
        let refused = send(
            &app,
            json_request(
                "POST",
                &convert_uri,
                Some(&cookie),
                json!({ "sold_on": "2026-11-06", "category_id": Uuid::new_v4() }),
            ),
        )
        .await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        let body = json!({ "sold_on": "2026-11-06", "category_id": category });
        let sale = send(
            &app,
            json_request("POST", &convert_uri, Some(&cookie), body.clone()),
        )
        .await;
        assert_eq!(sale.status, StatusCode::CREATED, "{}", sale.body);
        assert_eq!(sale.body["status"], "open");
        assert_eq!(sale.body["number"], 1);
        assert_eq!(sale.body["quote_id"], quote_id);
        assert_eq!(sale.body["total"], "35");
        assert_eq!(sale.body["payment_term_id"], targets.term);
        let sale_id = sale.body["id"].as_str().unwrap().to_owned();
        let quote_now = send(
            &app,
            get_request(
                &format!("{}/{quote_id}", quotes_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(quote_now.body["status"], "converted");
        // Asking again gives the same sale; the quote's link still shows it, approved.
        let again = send(
            &app,
            json_request("POST", &convert_uri, Some(&cookie), body),
        )
        .await;
        assert_eq!(again.body["id"], sale_id);
        let seen = send(&app, get_request(&format!("/public/quotes/{token}"), None)).await;
        assert_eq!(seen.body["status"], "converted");

        // Nothing is owed yet: an order has no receivable.
        let one = format!("{}/{sale_id}", sales_uri(company_id));
        let order = send(&app, get_request(&one, Some(&cookie))).await;
        assert_eq!(order.body["receivable_id"], Value::Null);
        let forecast =
            |from: &str, to: &str| cash_flow_uri(company_id, &format!("from={from}&to={to}"));
        let before = send(
            &app,
            get_request(&forecast("2026-12-01", "2027-01-31"), Some(&cookie)),
        )
        .await;
        assert_eq!(before.body["periods"][0]["to_receive"], "0");

        // Completed: the receivable is split by the term (30 and 60 days from the sale) into the cash flow.
        let done = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/complete"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(done.status, StatusCode::OK, "{}", done.body);
        assert_eq!(done.body["status"], "completed");
        let receivable_id = done.body["receivable_id"].as_str().unwrap().to_owned();
        let entry = send(
            &app,
            get_request(
                &format!("{}/{receivable_id}", entries_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(entry.body["sale_id"], sale_id);
        assert_eq!(entry.body["kind"], "receivable");
        assert_eq!(entry.body["description"], "Venda nº 1");
        assert_eq!(entry.body["party_id"], targets.customer);
        assert_eq!(entry.body["total"], "35");
        assert_eq!(
            entry.body["installments"][0],
            json!({ "number": 1, "due_date": "2026-12-06", "amount": "17.5", "settled": "0" })
        );
        assert_eq!(entry.body["installments"][1]["due_date"], "2027-01-05");
        let months = send(
            &app,
            get_request(&forecast("2026-12-01", "2027-01-31"), Some(&cookie)),
        )
        .await;
        assert_eq!(months.body["periods"][0]["to_receive"], "17.5");
        assert_eq!(months.body["periods"][1]["to_receive"], "17.5");
        assert_eq!(months.body["periods"][1]["projected_balance"], "35");

        // Its items are fixed; completing again changes nothing; the quote is fixed too.
        let locked = send(
            &app,
            json_request("PUT", &one, Some(&cookie), sale_body(&targets, &category)),
        )
        .await;
        assert_eq!(locked.status, StatusCode::CONFLICT);
        let twice = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/complete"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(twice.body["receivable_id"], receivable_id);
        let all = send(&app, get_request(&entries_uri(company_id), Some(&cookie))).await;
        assert_eq!(all.body["items"].as_array().unwrap().len(), 1);

        // Cancelled: the receivable goes with it, out of the cash flow, and stays listed as cancelled.
        let cancelled = send(
            &app,
            json_request("POST", &format!("{one}/cancel"), Some(&cookie), Value::Null),
        )
        .await;
        assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.body);
        assert_eq!(cancelled.body["status"], "cancelled");
        let after = send(
            &app,
            get_request(&forecast("2026-12-01", "2027-01-31"), Some(&cookie)),
        )
        .await;
        assert_eq!(after.body["periods"][0]["to_receive"], "0");
        assert_eq!(after.body["periods"][1]["to_receive"], "0");
        let voided = send(
            &app,
            get_request(
                &format!("{}?voided=true", entries_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(voided.body["items"][0]["voided"], true);
        let live = send(
            &app,
            get_request(
                &format!("{}?voided=false", entries_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert!(live.body["items"].as_array().unwrap().is_empty());
        let bad = send(
            &app,
            get_request(
                &format!("{}?voided=maybe", entries_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
        let history = send(&app, get_request(&format!("{one}/history"), Some(&cookie))).await;
        assert_eq!(history.body.as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn writes_changes_lists_and_returns_part_of_a_sale_through_the_api() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let category = revenue_category(&app, &cookie, company_id).await;
        let uri = sales_uri(company_id);

        let created = send(
            &app,
            json_request("POST", &uri, Some(&cookie), sale_body(&targets, &category)),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        assert_eq!(created.body["status"], "open");
        assert_eq!(created.body["lines"][0]["total"], "30");
        let id = created.body["id"].as_str().unwrap().to_owned();
        let one = format!("{uri}/{id}");
        for (field, value) in [
            ("seller_id", json!(targets.customer)),
            ("category_id", json!(Uuid::new_v4())),
            ("lines", json!([])),
        ] {
            let mut body = sale_body(&targets, &category);
            body[field] = value;
            assert_eq!(
                send(&app, json_request("POST", &uri, Some(&cookie), body))
                    .await
                    .status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{field}"
            );
        }
        let mut changed = sale_body(&targets, &category);
        changed["lines"][0]["quantity"] = json!("4");
        let replaced = send(&app, json_request("PUT", &one, Some(&cookie), changed)).await;
        assert_eq!(replaced.body["total"], "45.1");
        let found = send(
            &app,
            get_request(&format!("{uri}?q=franc%C3%AAs&status=open"), Some(&cookie)),
        )
        .await;
        assert_eq!(found.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            send(
                &app,
                get_request(&format!("{uri}?status=paid"), Some(&cookie))
            )
            .await
            .status,
            StatusCode::BAD_REQUEST
        );

        // An order has no returns.
        let returns_uri = format!("{one}/returns");
        let return_body = json!({
            "returned_on": "2026-11-10",
            "reason": "Estragou",
            "items": [{ "line": 1, "quantity": "1" }, { "line": 2, "quantity": "0.5" }],
            "refund_category_id": category,
            "refund_due_on": "2026-11-20",
        });
        let early = send(
            &app,
            json_request("POST", &returns_uri, Some(&cookie), return_body.clone()),
        )
        .await;
        assert_eq!(early.status, StatusCode::CONFLICT);
        let done = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/complete"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(done.status, StatusCode::OK, "{}", done.body);

        // 1 of 4 of a line worth 40.1 is 10.03, and a third of a line worth 5 is 1.67.
        let made = send(
            &app,
            json_request("POST", &returns_uri, Some(&cookie), return_body),
        )
        .await;
        assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
        assert_eq!(made.body["refund"], "11.7");
        assert_eq!(
            made.body["items"][1],
            json!({ "line": 2, "quantity": "0.5" })
        );
        let refund_id = made.body["refund_entry_id"].as_str().unwrap();
        let refund = send(
            &app,
            get_request(
                &format!("{}/{refund_id}", entries_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(refund.body["kind"], "payable");
        assert_eq!(refund.body["sale_return_id"], made.body["id"]);
        assert_eq!(refund.body["installments"][0]["due_date"], "2026-11-20");
        let too_much = send(
            &app,
            json_request(
                "POST",
                &returns_uri,
                Some(&cookie),
                json!({
                    "returned_on": "2026-11-11",
                    "items": [{ "line": 1, "quantity": "3.001" }],
                    "refund_category_id": category,
                    "refund_due_on": "2026-11-20",
                }),
            ),
        )
        .await;
        assert_eq!(too_much.status, StatusCode::CONFLICT);
        let listed = send(&app, get_request(&returns_uri, Some(&cookie))).await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
        // What was returned stands, so the sale cannot be cancelled.
        let cancel = send(
            &app,
            json_request("POST", &format!("{one}/cancel"), Some(&cookie), Value::Null),
        )
        .await;
        assert_eq!(cancel.status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn sales_follow_the_sales_role_and_stay_private_to_the_company() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let uri = sales_uri(company_id);
        for (role, may_write) in [
            (Role::Admin, true),
            (Role::Sales, true),
            (Role::Financial, false),
            (Role::Stock, false),
            (Role::Accountant, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&uri, Some(&cookie))).await.status,
                StatusCode::OK,
                "{role:?} reads"
            );
            let write = send(&app, json_request("POST", &uri, Some(&cookie), json!({}))).await;
            let expected = if may_write {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(write.status, expected, "{role:?} writes");
            let convert = send(
                &app,
                json_request(
                    "POST",
                    &format!("{}/{}/convert", quotes_uri(company_id), Uuid::new_v4()),
                    Some(&cookie),
                    json!({}),
                ),
            )
            .await;
            assert_eq!(
                convert.status,
                if may_write {
                    StatusCode::UNPROCESSABLE_ENTITY
                } else {
                    StatusCode::FORBIDDEN
                },
                "{role:?} converts"
            );
        }
        let (other, other_company) = sign_up(&app, "outra@example.com").await;
        let id = Uuid::new_v4();
        for path in ["", "/history", "/returns"] {
            let reply = send(
                &app,
                get_request(
                    &format!("{}/{id}{path}", sales_uri(other_company)),
                    Some(&other),
                ),
            )
            .await;
            assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        }
        assert_eq!(
            send(&app, get_request(&uri, Some(&other))).await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&uri, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    fn purchases_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/purchase-orders")
    }

    fn purchase_body(targets: &SalesTargets, supplier: &str, category: &str) -> Value {
        json!({
            "supplier_id": supplier,
            "ordered_on": "2026-11-06",
            "expected_on": "2026-11-20",
            "payment_term_id": targets.term,
            "category_id": category,
            "notes": "Entregar no depósito",
            "lines": [
                { "product_id": targets.product, "description": "Café 500g", "quantity": "12",
                  "unit_price": "8.10", "discount": "0.50" },
                { "product_id": targets.other_product, "description": "Pão francês", "quantity": "3",
                  "unit_price": "3.33" }
            ],
        })
    }

    #[tokio::test]
    async fn a_purchase_order_and_a_payment_receipt_are_mailed_to_the_party_through_the_queue() {
        let (app, mailbox) = entry_app_with_mailbox();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let category = revenue_category(&app, &cookie, company_id).await;
        let party = |name: &str, email: Option<&str>, supplier: bool| {
            let mut body =
                json!({ "name": name, "is_customer": !supplier, "is_supplier": supplier });
            if let Some(email) = email {
                body["email"] = json!(email);
            }
            send(
                &app,
                json_request("POST", &parties_uri(company_id), Some(&cookie), body),
            )
        };
        let supplier = party("Torrefação Boa", Some("fornecedor@example.com"), true).await;
        let supplier = supplier.body["id"].as_str().unwrap().to_owned();
        let silent = party("Sem Email", None, true).await.body["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let uri = purchases_uri(company_id);
        let order = |supplier: &str| {
            send(
                &app,
                json_request(
                    "POST",
                    &uri,
                    Some(&cookie),
                    purchase_body(&targets, supplier, &category),
                ),
            )
        };
        let mail = |id: &str, body: Value| {
            send(
                &app,
                json_request("POST", &format!("{uri}/{id}/send"), Some(&cookie), body),
            )
        };

        // A draft is not sent to a supplier; an approved order goes to the supplier's address.
        let draft = order(&supplier).await.body["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let refused = mail(&draft, json!({})).await;
        assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.body);
        send(
            &app,
            json_request(
                "POST",
                &format!("{uri}/{draft}/approve"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        let sent = mail(&draft, json!({})).await;
        assert_eq!(sent.status, StatusCode::OK, "{}", sent.body);
        assert_eq!(
            sent.body,
            json!({ "to": "fornecedor@example.com", "emailed": true })
        );
        let message = mailbox.sent().pop().unwrap();
        assert_eq!(message.to.as_str(), "fornecedor@example.com");
        assert_eq!(message.subject, "Pedido de compra nº 1 de Padaria");
        assert!(
            message.body.contains("Total: R$ 106,69"),
            "{}",
            message.body
        );
        assert!(message.body.contains("Itens:"), "{}", message.body);

        // The same order to the same address a moment later is refused, and what was sent is on record.
        let again = mail(&draft, json!({})).await;
        assert_eq!(
            again.status,
            StatusCode::TOO_MANY_REQUESTS,
            "{}",
            again.body
        );
        assert_eq!(mailbox.sent().len(), 1, "the refused one went nowhere");
        let record = send(
            &app,
            get_request(&format!("{uri}/{draft}/mails"), Some(&cookie)),
        )
        .await;
        assert_eq!(record.status, StatusCode::OK, "{}", record.body);
        assert_eq!(record.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(record.body["items"][0]["to"], "fornecedor@example.com");
        assert!(record.body["items"][0]["sent_at_millis"].as_i64().unwrap() > 0);

        // No address anywhere is a 422 that says what to do; an address can be given, and it must be one.
        let quiet = order(&silent).await.body["id"].as_str().unwrap().to_owned();
        send(
            &app,
            json_request(
                "POST",
                &format!("{uri}/{quiet}/approve"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(
            mail(&quiet, json!({})).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            mail(&quiet, json!({ "to": "not an address" })).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let given = mail(&quiet, json!({ "to": "compras@example.com" })).await;
        assert_eq!(given.body["to"], "compras@example.com");
        assert_eq!(
            mail("00000000-0000-0000-0000-000000000000", json!({}))
                .await
                .status,
            StatusCode::NOT_FOUND
        );

        // A settlement of a receivable is proved to the customer; a payable and a reversed payment are not.
        let account = registered_account(&app, &cookie, company_id, "Caixa").await;
        let customer = party("Mercadinho", Some("cliente@example.com"), false).await;
        let customer = customer.body["id"].as_str().unwrap().to_owned();
        let entries = entries_uri(company_id);
        let mut body = json!({
            "kind": "receivable", "description": "Venda de café", "party_id": customer,
            "category_id": category, "competence_date": "2026-11-05",
            "installments": [{ "due_date": "2026-12-10", "amount": "100" }, { "due_date": "2027-01-10", "amount": "50.5" }],
        });
        let entry = send(
            &app,
            json_request("POST", &entries, Some(&cookie), body.clone()),
        )
        .await;
        assert_eq!(entry.status, StatusCode::CREATED, "{}", entry.body);
        let entry = format!("{entries}/{}", entry.body["id"].as_str().unwrap());
        let settled = send(
            &app,
            json_request(
                "POST",
                &format!("{entry}/settlements"),
                Some(&cookie),
                json!({ "installment": 1, "settled_on": "2026-12-09", "applied": "100", "interest": "1.5", "bank_account_id": account }),
            ),
        )
        .await;
        assert_eq!(settled.status, StatusCode::CREATED, "{}", settled.body);
        let settlement = format!(
            "{entry}/settlements/{}",
            settled.body["id"].as_str().unwrap()
        );
        let receipt = send(
            &app,
            json_request(
                "POST",
                &format!("{settlement}/receipt"),
                Some(&cookie),
                json!({}),
            ),
        )
        .await;
        assert_eq!(receipt.status, StatusCode::OK, "{}", receipt.body);
        assert_eq!(
            receipt.body,
            json!({ "to": "cliente@example.com", "emailed": true })
        );
        let on_record = send(
            &app,
            get_request(&format!("{settlement}/mails"), Some(&cookie)),
        )
        .await;
        assert_eq!(on_record.body["items"][0]["to"], "cliente@example.com");
        // Sent again right away, it is refused.
        let twice = send(
            &app,
            json_request(
                "POST",
                &format!("{settlement}/receipt"),
                Some(&cookie),
                json!({}),
            ),
        )
        .await;
        assert_eq!(
            twice.status,
            StatusCode::TOO_MANY_REQUESTS,
            "{}",
            twice.body
        );
        let message = mailbox.sent().pop().unwrap();
        assert_eq!(message.subject, "Comprovante de pagamento de Padaria");
        for line in [
            "Olá, Mercadinho.",
            "Referente a: Venda de café",
            "Parcela: 1 de 2",
            "Data: 09/12/2026",
            "Juros: R$ 1,50",
            "Valor pago: R$ 101,50",
        ] {
            assert!(message.body.contains(line), "{line} in {}", message.body);
        }
        let reversed = send(
            &app,
            json_request("DELETE", &settlement, Some(&cookie), Value::Null),
        )
        .await;
        assert!(reversed.status.is_success(), "{}", reversed.body);
        assert_eq!(
            send(
                &app,
                json_request(
                    "POST",
                    &format!("{settlement}/receipt"),
                    Some(&cookie),
                    json!({})
                )
            )
            .await
            .status,
            StatusCode::CONFLICT
        );
        body["kind"] = json!("payable");
        let payable = send(&app, json_request("POST", &entries, Some(&cookie), body)).await;
        let payable = format!("{entries}/{}", payable.body["id"].as_str().unwrap());
        assert_eq!(
            send(
                &app,
                json_request(
                    "POST",
                    &format!(
                        "{payable}/settlements/{}/receipt",
                        settled.body["id"].as_str().unwrap()
                    ),
                    Some(&cookie),
                    json!({})
                )
            )
            .await
            .status,
            StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn a_purchase_order_is_approved_received_in_parts_and_pays_by_its_term() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let category = revenue_category(&app, &cookie, company_id).await;
        let supplier = send(
            &app,
            json_request(
                "POST",
                &parties_uri(company_id),
                Some(&cookie),
                json!({ "name": "Torrefação Boa", "is_customer": false, "is_supplier": true }),
            ),
        )
        .await;
        let supplier = supplier.body["id"].as_str().unwrap().to_owned();
        let uri = purchases_uri(company_id);

        let created = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                purchase_body(&targets, &supplier, &category),
            ),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        assert_eq!(created.body["status"], "draft");
        assert_eq!(created.body["number"], 1);
        assert_eq!(created.body["total"], "106.69");
        let id = created.body["id"].as_str().unwrap().to_owned();
        let one = format!("{uri}/{id}");

        // A customer is not a supplier, and the expected day is not before the order.
        let mut bad = purchase_body(&targets, &targets.customer, &category);
        assert_eq!(
            send(&app, json_request("POST", &uri, Some(&cookie), bad.clone()))
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        bad = purchase_body(&targets, &supplier, &category);
        bad["expected_on"] = json!("2026-11-05");
        assert_eq!(
            send(&app, json_request("POST", &uri, Some(&cookie), bad))
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let listed = send(
            &app,
            get_request(
                &format!("{uri}?q=franc%C3%AAs&status=draft&supplier={supplier}"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            send(
                &app,
                get_request(&format!("{uri}?status=paid"), Some(&cookie))
            )
            .await
            .status,
            StatusCode::BAD_REQUEST
        );

        // Nothing arrives at a draft: the state is the problem, not the request.
        let receipts_uri = format!("{one}/receipts");
        let partial = json!({
            "received_on": "2026-11-10",
            "items": [{ "line": 1, "quantity": "5" }, { "line": 2, "quantity": "3" }],
        });
        assert_eq!(
            send(
                &app,
                json_request("POST", &receipts_uri, Some(&cookie), partial.clone())
            )
            .await
            .status,
            StatusCode::CONFLICT
        );
        let approved = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/approve"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(approved.status, StatusCode::OK, "{}", approved.body);
        assert_eq!(approved.body["status"], "approved");
        assert_eq!(
            send(
                &app,
                json_request(
                    "PUT",
                    &one,
                    Some(&cookie),
                    purchase_body(&targets, &supplier, &category)
                )
            )
            .await
            .status,
            StatusCode::CONFLICT
        );

        // The first receipt: its payable is split by the term (30/60) from the day the goods arrived.
        let first = send(
            &app,
            json_request("POST", &receipts_uri, Some(&cookie), partial),
        )
        .await;
        assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
        assert_eq!(first.body["total"], "50.28");
        assert_eq!(first.body["items"][0]["value"], "40.29");
        let payable = first.body["payable_id"].as_str().unwrap().to_owned();
        let entry = send(
            &app,
            get_request(
                &format!("{}/{payable}", entries_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(entry.body["kind"], "payable");
        assert_eq!(entry.body["description"], "Compra nº 1");
        assert_eq!(entry.body["party_id"], supplier);
        assert_eq!(entry.body["purchase_receipt_id"], first.body["id"]);
        assert_eq!(entry.body["total"], "50.28");
        assert_eq!(entry.body["installments"][0]["due_date"], "2026-12-10");
        assert_eq!(entry.body["installments"][1]["due_date"], "2027-01-09");
        assert_eq!(
            send(&app, get_request(&one, Some(&cookie))).await.body["status"],
            "partially_received"
        );

        // More than what is left is refused and leaves nothing; a partly received order is not cancelled.
        let too_much = json!({
            "received_on": "2026-11-12",
            "items": [{ "line": 1, "quantity": "7.001" }],
        });
        assert_eq!(
            send(
                &app,
                json_request("POST", &receipts_uri, Some(&cookie), too_much)
            )
            .await
            .status,
            StatusCode::CONFLICT
        );
        assert_eq!(
            send(
                &app,
                json_request("POST", &format!("{one}/cancel"), Some(&cookie), Value::Null)
            )
            .await
            .status,
            StatusCode::CONFLICT
        );

        let rest = send(
            &app,
            json_request(
                "POST",
                &receipts_uri,
                Some(&cookie),
                json!({ "received_on": "2026-11-15", "items": [{ "line": 1, "quantity": "7" }] }),
            ),
        )
        .await;
        assert_eq!(rest.status, StatusCode::CREATED, "{}", rest.body);
        assert_eq!(rest.body["total"], "56.41");
        assert_eq!(
            send(&app, get_request(&one, Some(&cookie))).await.body["status"],
            "received"
        );
        let receipts = send(&app, get_request(&receipts_uri, Some(&cookie))).await;
        assert_eq!(receipts.body["items"].as_array().unwrap().len(), 2);
        let finished = send(
            &app,
            json_request(
                "POST",
                &format!(
                    "{receipts_uri}/{}/finish",
                    first.body["id"].as_str().unwrap()
                ),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(finished.status, StatusCode::OK, "{}", finished.body);
        assert_eq!(finished.body["payable_id"], payable.as_str());
        let payables = send(
            &app,
            get_request(
                &format!("{}?kind=payable", entries_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(payables.body["items"].as_array().unwrap().len(), 2);
        let history = send(&app, get_request(&format!("{one}/history"), Some(&cookie))).await;
        assert_eq!(history.body.as_array().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn purchases_follow_the_purchases_role_and_stay_private_to_the_company() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let uri = purchases_uri(company_id);
        for (role, may_write) in [
            (Role::Admin, true),
            (Role::Stock, true),
            (Role::Financial, false),
            (Role::Sales, false),
            (Role::Accountant, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&uri, Some(&cookie))).await.status,
                StatusCode::OK,
                "{role:?} reads"
            );
            let write = send(&app, json_request("POST", &uri, Some(&cookie), json!({}))).await;
            let expected = if may_write {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(write.status, expected, "{role:?} writes");
        }
        let (other, other_company) = sign_up(&app, "outra@example.com").await;
        let id = Uuid::new_v4();
        for path in ["", "/history", "/receipts"] {
            let reply = send(
                &app,
                get_request(
                    &format!("{}/{id}{path}", purchases_uri(other_company)),
                    Some(&other),
                ),
            )
            .await;
            assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        }
        assert_eq!(
            send(&app, get_request(&uri, Some(&other))).await.status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&uri, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    fn stock_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/stock")
    }

    #[tokio::test]
    async fn goods_received_enter_stock_and_a_sale_takes_them_at_the_average_cost() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let category = revenue_category(&app, &cookie, company_id).await;
        let supplier = send(
            &app,
            json_request(
                "POST",
                &parties_uri(company_id),
                Some(&cookie),
                json!({ "name": "Torrefação Boa", "is_customer": false, "is_supplier": true }),
            ),
        )
        .await
        .body["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let positions_uri = format!("{}/positions", stock_uri(company_id));
        let position_of = |positions: &Value, product: &str| -> Value {
            positions["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["product_id"] == product)
                .unwrap()
                .clone()
        };

        // Nothing is on hand before anything arrives.
        let none = send(&app, get_request(&positions_uri, Some(&cookie))).await;
        assert_eq!(position_of(&none.body, &targets.product)["quantity"], "0");

        // A purchase of 12 coffees (96.70 after the discount) and 3 breads (9.99) is received whole.
        let order = send(
            &app,
            json_request(
                "POST",
                &purchases_uri(company_id),
                Some(&cookie),
                purchase_body(&targets, &supplier, &category),
            ),
        )
        .await;
        let order_uri = format!(
            "{}/{}",
            purchases_uri(company_id),
            order.body["id"].as_str().unwrap()
        );
        send(
            &app,
            json_request(
                "POST",
                &format!("{order_uri}/approve"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        let received = send(
            &app,
            json_request(
                "POST",
                &format!("{order_uri}/receipts"),
                Some(&cookie),
                json!({
                    "received_on": "2026-11-10",
                    "items": [{ "line": 1, "quantity": "12" }, { "line": 2, "quantity": "3" }],
                }),
            ),
        )
        .await;
        assert_eq!(received.status, StatusCode::CREATED, "{}", received.body);
        let positions = send(&app, get_request(&positions_uri, Some(&cookie))).await;
        let coffee = position_of(&positions.body, &targets.product);
        assert_eq!(
            (
                &coffee["quantity"],
                &coffee["value"],
                &coffee["average_cost"]
            ),
            (&json!("12"), &json!("96.7"), &json!("8.058333"))
        );

        // A sale of 3 coffees and 1.5 breads, completed: the coffees leave at the average (96.70 x 3/12 = 24.175,
        // which rounds up to 24.18) and the breads at theirs (4.995 -> 5).
        let sale = send(
            &app,
            json_request(
                "POST",
                &sales_uri(company_id),
                Some(&cookie),
                sale_body(&targets, &category),
            ),
        )
        .await;
        let sale_uri = format!(
            "{}/{}",
            sales_uri(company_id),
            sale.body["id"].as_str().unwrap()
        );
        let completed = send(
            &app,
            json_request(
                "POST",
                &format!("{sale_uri}/complete"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(completed.status, StatusCode::OK, "{}", completed.body);
        let positions = send(&app, get_request(&positions_uri, Some(&cookie))).await;
        let coffee = position_of(&positions.body, &targets.product);
        assert_eq!(
            (&coffee["quantity"], &coffee["value"]),
            (&json!("9"), &json!("72.52"))
        );
        let bread = position_of(&positions.body, &targets.other_product);
        assert_eq!(
            (&bread["quantity"], &bread["value"]),
            (&json!("1.5"), &json!("4.99"))
        );
        // The ledger says what moved and why; what was before a day is what the position of that day says.
        let ledger = send(
            &app,
            get_request(
                &format!(
                    "{}/movements?kind=out&product={}",
                    stock_uri(company_id),
                    targets.product
                ),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(ledger.body["items"][0]["quantity"], "-3");
        assert_eq!(ledger.body["items"][0]["value"], "-24.18");
        assert_eq!(ledger.body["items"][0]["origin"]["kind"], "sale");
        let before = send(
            &app,
            get_request(&format!("{positions_uri}?on=2026-11-09"), Some(&cookie)),
        )
        .await;
        // The sale is dated the 6th and the goods came on the 10th: on the 9th the shelf was short by what it sold.
        assert_eq!(
            position_of(&before.body, &targets.product)["quantity"],
            "-3"
        );

        // By hand: an exit beyond what there is is refused with a clear 409, an adjustment is valued at the
        // average, and a movement is undone once.
        let movements = format!("{}/movements", stock_uri(company_id));
        let too_much = send(
            &app,
            json_request(
                "POST",
                &movements,
                Some(&cookie),
                json!({ "product_id": targets.product, "kind": "out", "quantity": "10",
                        "occurred_on": "2026-11-11" }),
            ),
        )
        .await;
        assert_eq!(too_much.status, StatusCode::CONFLICT);
        assert_eq!(
            too_much.body["error"],
            "not enough stock of Café: 9 on hand"
        );
        let found = send(
            &app,
            json_request(
                "POST",
                &movements,
                Some(&cookie),
                json!({ "product_id": targets.product, "kind": "adjustment", "quantity": "-1",
                        "occurred_on": "2026-11-11", "note": "Quebrou" }),
            ),
        )
        .await;
        assert_eq!(found.status, StatusCode::CREATED, "{}", found.body);
        let undo = format!("{movements}/{}/reverse", found.body["id"].as_str().unwrap());
        let body = json!({ "occurred_on": "2026-11-12" });
        let undone = send(
            &app,
            json_request("POST", &undo, Some(&cookie), body.clone()),
        )
        .await;
        assert_eq!(undone.status, StatusCode::CREATED, "{}", undone.body);
        assert_eq!(
            send(
                &app,
                json_request("POST", &undo, Some(&cookie), body.clone())
            )
            .await
            .status,
            StatusCode::CONFLICT
        );
        // What a sale made is undone through the sale, a service has no stock, and bad input is a 422.
        let sale_exit = ledger.body["items"][0]["id"].as_str().unwrap();
        assert_eq!(
            send(
                &app,
                json_request(
                    "POST",
                    &format!("{movements}/{sale_exit}/reverse"),
                    Some(&cookie),
                    body
                )
            )
            .await
            .status,
            StatusCode::CONFLICT
        );
        for bad in [
            json!({ "product_id": targets.product, "kind": "sideways", "quantity": "1", "occurred_on": "2026-11-11" }),
            json!({ "product_id": targets.product, "kind": "in", "quantity": "1", "occurred_on": "2026-11-11" }),
            json!({ "product_id": targets.product, "kind": "out", "quantity": "-1", "occurred_on": "2026-11-11" }),
        ] {
            assert_eq!(
                send(&app, json_request("POST", &movements, Some(&cookie), bad))
                    .await
                    .status,
                StatusCode::UNPROCESSABLE_ENTITY
            );
        }

        // A cancelled sale gives its exits back.
        send(
            &app,
            json_request(
                "POST",
                &format!("{sale_uri}/cancel"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        let positions = send(&app, get_request(&positions_uri, Some(&cookie))).await;
        let coffee = position_of(&positions.body, &targets.product);
        assert_eq!(
            (&coffee["quantity"], &coffee["value"]),
            (&json!("12"), &json!("96.7"))
        );
        assert_eq!(
            send(
                &app,
                get_request(&format!("{movements}?kind=sideways"), Some(&cookie))
            )
            .await
            .status,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn minimum_stock_alerts_and_the_block_of_a_sale_without_balance() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let category = revenue_category(&app, &cookie, company_id).await;
        let alerts_uri = format!("{}/alerts", stock_uri(company_id));
        let settings_uri = format!("{}/settings", stock_uri(company_id));
        let movements = format!("{}/movements", stock_uri(company_id));

        // Both products have a minimum of 10 and nothing on hand.
        let alerts = send(&app, get_request(&alerts_uri, Some(&cookie))).await;
        assert_eq!(alerts.status, StatusCode::OK, "{}", alerts.body);
        let names: Vec<&str> = alerts.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Café", "Pão"]);
        assert_eq!(alerts.body["items"][0]["shortfall"], "10");
        let enter = |product: &str, quantity: &str| {
            json!({ "product_id": product, "kind": "in", "quantity": quantity,
                    "unit_cost": "5", "occurred_on": "2026-11-01" })
        };
        for (product, quantity) in [(&targets.product, "10"), (&targets.other_product, "1")] {
            let made = send(
                &app,
                json_request("POST", &movements, Some(&cookie), enter(product, quantity)),
            )
            .await;
            assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
        }
        let alerts = send(&app, get_request(&alerts_uri, Some(&cookie))).await;
        let items = alerts.body["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            (
                &items[0]["name"],
                &items[0]["quantity"],
                &items[0]["shortfall"]
            ),
            (&json!("Pão"), &json!("1"), &json!("9"))
        );

        // The setting is off until the company says so, and saying it is recorded.
        let settings = send(&app, get_request(&settings_uri, Some(&cookie))).await;
        assert_eq!(settings.body["block_sale_without_balance"], false);
        let turned_on = send(
            &app,
            json_request(
                "PUT",
                &settings_uri,
                Some(&cookie),
                json!({ "block_sale_without_balance": true }),
            ),
        )
        .await;
        assert_eq!(turned_on.status, StatusCode::OK, "{}", turned_on.body);
        assert_eq!(turned_on.body["block_sale_without_balance"], true);
        assert_eq!(
            send(
                &app,
                json_request("PUT", &settings_uri, Some(&cookie), json!({}))
            )
            .await
            .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let history = send(
            &app,
            get_request(&format!("{settings_uri}/history"), Some(&cookie)),
        )
        .await;
        assert_eq!(history.body.as_array().unwrap().len(), 1);

        // A sale of 3 coffees and 1.5 breads, with 1 bread on hand: refused with a clear 409, naming what is short,
        // and the order stays open.
        let sale = send(
            &app,
            json_request(
                "POST",
                &sales_uri(company_id),
                Some(&cookie),
                sale_body(&targets, &category),
            ),
        )
        .await;
        let sale_uri = format!(
            "{}/{}",
            sales_uri(company_id),
            sale.body["id"].as_str().unwrap()
        );
        let refused = send(
            &app,
            json_request(
                "POST",
                &format!("{sale_uri}/complete"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(refused.status, StatusCode::CONFLICT);
        assert_eq!(refused.body["error"], "not enough stock of Pão: 1 on hand");
        assert_eq!(
            send(&app, get_request(&sale_uri, Some(&cookie))).await.body["status"],
            "open"
        );
        // Without the block the same sale is made and leaves the bread below zero.
        send(
            &app,
            json_request(
                "PUT",
                &settings_uri,
                Some(&cookie),
                json!({ "block_sale_without_balance": false }),
            ),
        )
        .await;
        let done = send(
            &app,
            json_request(
                "POST",
                &format!("{sale_uri}/complete"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(done.status, StatusCode::OK, "{}", done.body);
        let positions = send(
            &app,
            get_request(
                &format!("{}/positions", stock_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        let bread = positions.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["product_id"] == targets.other_product)
            .unwrap();
        assert_eq!(bread["quantity"], "-0.5");
    }

    #[tokio::test]
    async fn stock_reports_value_the_stock_on_a_day_and_summarize_its_movements() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let movements = format!("{}/movements", stock_uri(company_id));
        for body in [
            json!({ "product_id": targets.product, "kind": "in", "quantity": "10",
                    "unit_cost": "5", "occurred_on": "2026-11-01" }),
            json!({ "product_id": targets.product, "kind": "out", "quantity": "4",
                    "occurred_on": "2026-11-06" }),
            json!({ "product_id": targets.other_product, "kind": "in", "quantity": "3",
                    "value": "9.99", "occurred_on": "2026-11-10" }),
        ] {
            let made = send(&app, json_request("POST", &movements, Some(&cookie), body)).await;
            assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
        }
        let reports = format!("/companies/{company_id}/stock-reports");

        // 10 at 5 less 4 at the average (20): 6 worth 30; the bread joins on the 10th.
        let on_6th = send(
            &app,
            get_request(&format!("{reports}/valuation?on=2026-11-06"), Some(&cookie)),
        )
        .await;
        assert_eq!(on_6th.status, StatusCode::OK, "{}", on_6th.body);
        assert_eq!(on_6th.body["total"], "30");
        assert_eq!(on_6th.body["rows"].as_array().unwrap().len(), 1);
        assert_eq!(on_6th.body["rows"][0]["average_cost"], "5");
        assert_eq!(on_6th.body["rows"][0]["share"], "100");
        let on_10th = send(
            &app,
            get_request(&format!("{reports}/valuation?on=2026-11-10"), Some(&cookie)),
        )
        .await;
        assert_eq!(on_10th.body["total"], "39.99");
        assert_eq!(on_10th.body["rows"][0]["name"], "Café");
        assert_eq!(on_10th.body["rows"][0]["share"], "75.02");
        assert_eq!(on_10th.body["rows"][1]["share"], "24.98");

        let summary = send(
            &app,
            get_request(
                &format!("{reports}/movements?from=2026-11-05&to=2026-11-30"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(summary.status, StatusCode::OK, "{}", summary.body);
        let coffee = &summary.body["items"][0];
        assert_eq!(coffee["name"], "Café");
        assert_eq!(
            (
                &coffee["opening_quantity"],
                &coffee["exited_quantity"],
                &coffee["exited_value"],
                &coffee["closing_value"]
            ),
            (&json!("10"), &json!("4"), &json!("20"), &json!("30"))
        );
        assert_eq!(summary.body["items"][1]["entered_value"], "9.99");

        // The day is required and a span is bounded; both are the client's to say.
        for (uri, status) in [
            (format!("{reports}/valuation"), StatusCode::BAD_REQUEST),
            (
                format!("{reports}/valuation?on=soon"),
                StatusCode::BAD_REQUEST,
            ),
            (
                format!("{reports}/movements?from=2026-11-30&to=2026-11-05"),
                StatusCode::BAD_REQUEST,
            ),
            (
                format!("{reports}/movements?from=2024-01-01&to=2026-11-05"),
                StatusCode::BAD_REQUEST,
            ),
        ] {
            assert_eq!(
                send(&app, get_request(&uri, Some(&cookie))).await.status,
                status,
                "{uri}"
            );
        }
        assert_eq!(
            send(
                &app,
                get_request(&format!("{reports}/valuation?on=2026-11-06"), None)
            )
            .await
            .status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn stock_follows_the_purchases_role_and_stays_private_to_the_company() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let movements = format!("{}/movements", stock_uri(company_id));
        for (role, may_write) in [
            (Role::Admin, true),
            (Role::Stock, true),
            (Role::Financial, false),
            (Role::Sales, false),
            (Role::Accountant, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            for uri in [&movements, &format!("{}/positions", stock_uri(company_id))] {
                assert_eq!(
                    send(&app, get_request(uri, Some(&cookie))).await.status,
                    StatusCode::OK,
                    "{role:?} reads"
                );
            }
            let write = send(
                &app,
                json_request("POST", &movements, Some(&cookie), json!({})),
            )
            .await;
            let expected = if may_write {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(write.status, expected, "{role:?} writes");
        }
        let (other, other_company) = sign_up(&app, "outra@example.com").await;
        assert_eq!(
            send(
                &app,
                get_request(
                    &format!("{}/movements/{}", stock_uri(other_company), Uuid::new_v4()),
                    Some(&other)
                )
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&movements, Some(&other)))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get_request(&movements, None)).await.status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn a_count_is_looked_at_against_the_stock_and_applied_as_adjustments() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let movements = format!("{}/movements", stock_uri(company_id));
        let counts = format!("/companies/{company_id}/inventory-counts");
        let entered = send(
            &app,
            json_request(
                "POST",
                &movements,
                Some(&cookie),
                json!({ "product_id": targets.product, "kind": "in", "quantity": "10",
                        "unit_cost": "5", "occurred_on": "2026-11-01" }),
            ),
        )
        .await;
        assert_eq!(entered.status, StatusCode::CREATED, "{}", entered.body);

        // Written open; a product that has no stock cannot be counted and a quantity is not negative.
        let body = json!({
            "counted_on": "2026-11-30",
            "notes": "Fim do mês",
            "lines": [{ "product_id": targets.product, "counted": "8" }],
        });
        let created = send(
            &app,
            json_request("POST", &counts, Some(&cookie), body.clone()),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        assert_eq!(created.body["status"], "open");
        assert_eq!(created.body["number"], 1);
        for (bad, status) in [
            (
                json!({ "counted_on": "2026-11-30", "lines": [] }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "counted_on": "2026-11-30",
                        "lines": [{ "product_id": targets.product, "counted": "-1" }] }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "counted_on": "2026-11-30",
                        "lines": [{ "product_id": Uuid::new_v4(), "counted": "1" }] }),
                StatusCode::NOT_FOUND,
            ),
        ] {
            assert_eq!(
                send(&app, json_request("POST", &counts, Some(&cookie), bad))
                    .await
                    .status,
                status
            );
        }
        let one = format!("{counts}/{}", created.body["id"].as_str().unwrap());
        let seen = send(&app, get_request(&one, Some(&cookie))).await;
        assert_eq!(
            (
                &seen.body["lines"][0]["counted"],
                &seen.body["lines"][0]["expected"],
                &seen.body["lines"][0]["difference"]
            ),
            (&json!("8"), &json!("10"), &json!("-2"))
        );

        // Applied: the coffees are adjusted at the average (-2 of 10 worth 50 is -10), once.
        let applied = send(
            &app,
            json_request("POST", &format!("{one}/apply"), Some(&cookie), Value::Null),
        )
        .await;
        assert_eq!(applied.status, StatusCode::OK, "{}", applied.body);
        assert_eq!(applied.body["status"], "applied");
        assert_eq!(applied.body["lines"][0]["difference"], "-2");
        let ledger = send(
            &app,
            get_request(&format!("{movements}?kind=adjustment"), Some(&cookie)),
        )
        .await;
        assert_eq!(ledger.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(ledger.body["items"][0]["value"], "-10");
        assert_eq!(ledger.body["items"][0]["origin"]["kind"], "inventory_count");
        let again = send(
            &app,
            json_request("POST", &format!("{one}/apply"), Some(&cookie), Value::Null),
        )
        .await;
        assert_eq!(again.status, StatusCode::OK);
        let ledger = send(
            &app,
            get_request(&format!("{movements}?kind=adjustment"), Some(&cookie)),
        )
        .await;
        assert_eq!(ledger.body["items"].as_array().unwrap().len(), 1);
        // It is fixed, and what is not open cannot be cancelled.
        for reply in [
            send(&app, json_request("PUT", &one, Some(&cookie), body)).await,
            send(
                &app,
                json_request("POST", &format!("{one}/cancel"), Some(&cookie), Value::Null),
            )
            .await,
        ] {
            assert_eq!(reply.status, StatusCode::CONFLICT);
        }
        let positions = send(
            &app,
            get_request(
                &format!("{}/positions", stock_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        let coffee = positions.body["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["product_id"] == targets.product)
            .unwrap();
        assert_eq!(
            (&coffee["quantity"], &coffee["value"]),
            (&json!("8"), &json!("40"))
        );
        let history = send(&app, get_request(&format!("{one}/history"), Some(&cookie))).await;
        assert_eq!(history.body.as_array().unwrap().len(), 2);
        let listed = send(
            &app,
            get_request(&format!("{counts}?status=applied&q=fim"), Some(&cookie)),
        )
        .await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            send(
                &app,
                get_request(&format!("{counts}?status=done"), Some(&cookie))
            )
            .await
            .status,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn counts_follow_the_purchases_role_and_stay_private_to_the_company() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let uri = format!("/companies/{company_id}/inventory-counts");
        for (role, may_write) in [
            (Role::Admin, true),
            (Role::Stock, true),
            (Role::Financial, false),
            (Role::Sales, false),
            (Role::Accountant, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&uri, Some(&cookie))).await.status,
                StatusCode::OK,
                "{role:?} reads"
            );
            let write = send(&app, json_request("POST", &uri, Some(&cookie), json!({}))).await;
            let expected = if may_write {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(write.status, expected, "{role:?} writes");
        }
        let (other, other_company) = sign_up(&app, "outra@example.com").await;
        let id = Uuid::new_v4();
        for path in ["", "/history"] {
            let reply = send(
                &app,
                get_request(
                    &format!("/companies/{other_company}/inventory-counts/{id}{path}"),
                    Some(&other),
                ),
            )
            .await;
            assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        }
        assert_eq!(
            send(&app, get_request(&uri, Some(&other))).await.status,
            StatusCode::NOT_FOUND
        );
    }

    /// An NF-e of the issuer 11.222.333/0001-81 (series 1, number 4321) with two items and two installments, whose
    /// access key has the right check digit.
    fn invoice_xml() -> String {
        let key = "35261111222333000181550010000043211123456781";
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe" versao="4.00"><NFe>
<infNFe Id="NFe{key}" versao="4.00">
<ide><mod>55</mod><serie>1</serie><nNF>4321</nNF><dhEmi>2026-11-06T10:30:00-03:00</dhEmi><finNFe>1</finNFe></ide>
<emit><CNPJ>11222333000181</CNPJ><xNome>Torrefação Boa Ltda</xNome></emit>
<det nItem="1"><prod><cProd>CAFE-500</cProd><xProd>Café 500g</xProd><NCM>09012100</NCM><uCom>UN</uCom><qCom>10.0000</qCom><vUnCom>10.00</vUnCom><vProd>100.00</vProd></prod>
<imposto><IPI><IPITrib><vIPI>2.50</vIPI></IPITrib></IPI></imposto></det>
<det nItem="2"><prod><cProd>PAO-1</cProd><xProd>Pão francês</xProd><NCM>19059090</NCM><uCom>KG</uCom><qCom>3.0000</qCom><vUnCom>3.33</vUnCom><vProd>9.99</vProd><vDesc>0.99</vDesc></prod><imposto/></det>
<total><ICMSTot><vProd>109.99</vProd><vDesc>0.99</vDesc><vIPI>2.50</vIPI><vNF>111.50</vNF></ICMSTot></total>
<cobr><dup><nDup>001</nDup><dVenc>2026-12-06</dVenc><vDup>55.75</vDup></dup><dup><nDup>002</nDup><dVenc>2027-01-06</dVenc><vDup>55.75</vDup></dup></cobr>
</infNFe></NFe></nfeProc>"#
        )
    }

    #[tokio::test]
    async fn a_purchase_xml_creates_payables_and_stock_and_a_sale_takes_it_at_the_average_cost() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let category = revenue_category(&app, &cookie, company_id).await;
        let supplier = send(
            &app,
            json_request(
                "POST",
                &parties_uri(company_id),
                Some(&cookie),
                json!({ "name": "Torrefação Boa", "document": "11.222.333/0001-81",
                        "is_customer": true, "is_supplier": true }),
            ),
        )
        .await;
        assert_eq!(supplier.status, StatusCode::CREATED, "{}", supplier.body);
        let supplier = supplier.body["id"].as_str().unwrap().to_owned();
        let mut product_ids = Vec::new();
        for (name, sku) in [("Café 500g", "CAFE-500"), ("Pão francês", "PAO-1")] {
            let mut body = product_body();
            body["name"] = json!(name);
            body["sku"] = json!(sku);
            let made = post_product(&app, &cookie, company_id, body).await;
            assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
            product_ids.push(made.body["id"].as_str().unwrap().to_owned());
        }
        let imports = format!("/companies/{company_id}/purchase-imports");
        let post = |uri: String, xml: String| {
            let app = app.clone();
            let cookie = cookie.clone();
            async move {
                send(
                    &app,
                    file_request(
                        "POST",
                        &uri,
                        Some(&cookie),
                        "application/xml",
                        xml.into_bytes(),
                    ),
                )
                .await
            }
        };

        // Looked at first: what the invoice is, its supplier and the products its codes find.
        let preview = post(format!("{imports}/preview"), invoice_xml()).await;
        assert_eq!(preview.status, StatusCode::OK, "{}", preview.body);
        assert_eq!(preview.body["total"], "111.5");
        assert_eq!(preview.body["supplier_id"], supplier);
        assert_eq!(preview.body["items"][0]["product_id"], product_ids[0]);
        assert_eq!(preview.body["items"][0]["cost"], "102.5");
        assert_eq!(preview.body["items"][1]["cost"], "9");
        assert_eq!(preview.body["installments"].as_array().unwrap().len(), 2);
        assert!(preview.body["entered"].is_null());

        // Entered: a purchase received whole, a payable on the invoice's installments, the goods in stock.
        let uri = format!("{imports}?category_id={category}&received_on=2026-11-10");
        let entered = post(uri.clone(), invoice_xml()).await;
        assert_eq!(entered.status, StatusCode::CREATED, "{}", entered.body);
        assert_eq!(entered.body["status"], "received");
        assert_eq!(entered.body["already_entered"], false);
        let payable = entered.body["receipt"]["payable_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let entry = send(
            &app,
            get_request(
                &format!("{}/{payable}", entries_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(entry.body["kind"], "payable");
        assert_eq!(entry.body["party_id"], supplier);
        assert_eq!(entry.body["total"], "111.5");
        assert_eq!(entry.body["installments"][0]["due_date"], "2026-12-06");
        assert_eq!(entry.body["installments"][1]["due_date"], "2027-01-06");
        let positions = format!("/companies/{company_id}/stock/positions");
        let stock = send(&app, get_request(&positions, Some(&cookie))).await;
        let position = |body: &Value, product: &str| -> Value {
            body["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["product_id"] == product)
                .unwrap()
                .clone()
        };
        let coffee = position(&stock.body, &product_ids[0]);
        assert_eq!(
            (
                &coffee["quantity"],
                &coffee["value"],
                &coffee["average_cost"]
            ),
            (&json!("10"), &json!("102.5"), &json!("10.25"))
        );

        // The tiles: what the products hold per situation, and the purchases per status.
        let summary = send(
            &app,
            get_request(&format!("{positions}/summary"), Some(&cookie)),
        )
        .await;
        assert_eq!(summary.status, StatusCode::OK, "{}", summary.body);
        assert_eq!(summary.body["value"], "111.5");
        assert!(summary.body["in_stock"].as_u64().unwrap() >= 2);
        let depleted = send(
            &app,
            get_request(&format!("{positions}?situation=depleted"), Some(&cookie)),
        )
        .await;
        assert!(
            depleted.body["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item["product_id"] != product_ids[0])
        );
        let refused = send(
            &app,
            get_request(&format!("{positions}?situation=none"), Some(&cookie)),
        )
        .await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST);
        let counts = send(
            &app,
            get_request(
                &format!("/companies/{company_id}/purchase-orders/status-counts"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(counts.status, StatusCode::OK, "{}", counts.body);
        assert_eq!(
            counts.body,
            json!({
                "draft": 0,
                "approved": 0,
                "partially_received": 0,
                "received": 1,
                "cancelled": 0
            })
        );

        // The same file again changes nothing.
        let again = post(uri, invoice_xml()).await;
        assert_eq!(again.status, StatusCode::OK, "{}", again.body);
        assert_eq!(again.body["already_entered"], true);
        assert_eq!(again.body["purchase_id"], entered.body["purchase_id"]);
        let payables = send(
            &app,
            get_request(
                &format!("{}?kind=payable", entries_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(payables.body["items"].as_array().unwrap().len(), 1);
        let preview = post(format!("{imports}/preview"), invoice_xml()).await;
        assert_eq!(preview.body["entered"]["status"], "received");

        // A sale of 4 coffees takes them at the average: 102.50 x 4/10 = 41.00.
        let customer = send(
            &app,
            json_request(
                "POST",
                &parties_uri(company_id),
                Some(&cookie),
                json!({ "name": "Cliente", "is_customer": true, "is_supplier": false }),
            ),
        )
        .await;
        let sale = send(
            &app,
            json_request(
                "POST",
                &sales_uri(company_id),
                Some(&cookie),
                json!({
                    "customer_id": customer.body["id"],
                    "sold_on": "2026-11-12",
                    "category_id": category,
                    "lines": [{ "product_id": product_ids[0], "description": "Café 500g",
                                "quantity": "4", "unit_price": "15" }],
                }),
            ),
        )
        .await;
        assert_eq!(sale.status, StatusCode::CREATED, "{}", sale.body);
        let sale_uri = format!(
            "{}/{}",
            sales_uri(company_id),
            sale.body["id"].as_str().unwrap()
        );
        let done = send(
            &app,
            json_request(
                "POST",
                &format!("{sale_uri}/complete"),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        assert_eq!(done.status, StatusCode::OK, "{}", done.body);
        let stock = send(&app, get_request(&positions, Some(&cookie))).await;
        let coffee = position(&stock.body, &product_ids[0]);
        assert_eq!(
            (
                &coffee["quantity"],
                &coffee["value"],
                &coffee["average_cost"]
            ),
            (&json!("6"), &json!("61.5"), &json!("10.25"))
        );

        // What is not an NF-e, or is hostile, is refused with a message; an issuer nobody registered is too.
        for (xml, status) in [
            ("not xml".to_owned(), StatusCode::UNPROCESSABLE_ENTITY),
            (
                "<!DOCTYPE a [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><a>&x;</a>".to_owned(),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                invoice_xml().replace("<vNF>111.50</vNF>", "<vNF>111.51</vNF>"),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            assert_eq!(post(format!("{imports}/preview"), xml).await.status, status);
        }
        // An issuer nobody registered: the key says who issued it, so the file is another invoice altogether, and
        // one whose key is right still has no supplier to be entered for.
        let stranger = invoice_xml().replace("11222333000181", "11444777000161");
        assert_eq!(
            post(format!("{imports}?category_id={category}"), stranger)
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let big = post(format!("{imports}/preview"), " ".repeat(1024 * 1024 + 10)).await;
        assert_eq!(big.status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            post(format!("{imports}?received_on=2026-11-10"), invoice_xml())
                .await
                .status,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn invoice_imports_follow_the_purchases_role() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let imports = format!("/companies/{company_id}/purchase-imports");
        for (role, may_write) in [
            (Role::Admin, true),
            (Role::Stock, true),
            (Role::Financial, false),
            (Role::Sales, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            let reply = send(
                &app,
                file_request(
                    "POST",
                    &format!("{imports}/preview"),
                    Some(&cookie),
                    "application/xml",
                    b"not xml".to_vec(),
                ),
            )
            .await;
            assert_eq!(
                reply.status,
                if may_write {
                    StatusCode::UNPROCESSABLE_ENTITY
                } else {
                    StatusCode::FORBIDDEN
                },
                "{role:?}"
            );
        }
    }

    fn contracts_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/contracts")
    }

    fn contract_body(targets: &SalesTargets, category: &str) -> Value {
        json!({
            "name": "Manutenção mensal",
            "customer_id": targets.customer,
            "payment_term_id": targets.term,
            "category_id": category,
            "lines": [
                { "product_id": targets.product, "description": "Suporte", "quantity": "2",
                  "unit_price": "100.10", "discount": "0.20" }
            ],
            "cycle": "monthly",
            "starts_on": "2026-09-05",
            "ends_on": "2026-11-30",
        })
    }

    #[tokio::test]
    async fn a_contract_bills_itself_every_cycle_into_sales_and_receivables() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let category = revenue_category(&app, &cookie, company_id).await;
        let uri = contracts_uri(company_id);

        // Written with its first cycles already past: three fall on 5 September, 5 October and 5 November.
        let created = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                contract_body(&targets, &category),
            ),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        assert_eq!(created.body["status"], "active");
        assert_eq!(created.body["total"], "200");
        // The server billed through the day it was written (or the last day of the contract, once past).
        assert!(created.body["billed_through"].is_string());
        let id = created.body["id"].as_str().unwrap().to_owned();
        let one = format!("{uri}/{id}");
        assert_eq!(
            send(&app, get_request(&one, Some(&cookie))).await.body,
            created.body
        );
        let billed = send(
            &app,
            get_request(
                &format!("{}?contract={id}", sales_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        let items = billed.body["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert!(items.iter().all(|sale| sale["status"] == "completed"
            && sale["contract_id"] == id
            && sale["total"] == "200"));
        let mut days: Vec<&str> = items
            .iter()
            .map(|sale| sale["cycle_on"].as_str().unwrap())
            .collect();
        days.sort_unstable();
        assert_eq!(days, ["2026-09-05", "2026-10-05", "2026-11-05"]);
        // Each cycle's receivable is in the entries, split by the term, and in the cash flow forecast.
        let entries = send(&app, get_request(&entries_uri(company_id), Some(&cookie))).await;
        assert_eq!(entries.body["items"].as_array().unwrap().len(), 3);
        let flow = send(
            &app,
            get_request(
                &cash_flow_uri(company_id, "from=2026-10-01&to=2027-01-31"),
                Some(&cookie),
            ),
        )
        .await;
        let receivable: Vec<&str> = flow.body["periods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|period| period["to_receive"].as_str().unwrap())
            .collect();
        // 30 and 60 days after each cycle, 100 each: October gets 5 September's first (5 October), November
        // its second (4 November) and 5 October's first, December 5 October's second and 5 November's first.
        assert_eq!(receivable, ["100", "200", "200", "100"]);

        // An adjustment is audited and changes what the next cycles bill; a replacement says whether it is active.
        let raised = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/adjust"),
                Some(&cookie),
                json!({ "percent": "10" }),
            ),
        )
        .await;
        assert_eq!(raised.status, StatusCode::OK, "{}", raised.body);
        assert_eq!(raised.body["total"], "220.02");
        let bad = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/adjust"),
                Some(&cookie),
                json!({ "percent": "-100" }),
            ),
        )
        .await;
        assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
        let mut replaced = contract_body(&targets, &category);
        let missing = send(
            &app,
            json_request("PUT", &one, Some(&cookie), replaced.clone()),
        )
        .await;
        assert_eq!(missing.status, StatusCode::UNPROCESSABLE_ENTITY);
        replaced["active"] = json!(false);
        let paused = send(&app, json_request("PUT", &one, Some(&cookie), replaced)).await;
        assert_eq!(paused.status, StatusCode::OK, "{}", paused.body);
        assert_eq!(paused.body["status"], "paused");
        let history = send(&app, get_request(&format!("{one}/history"), Some(&cookie))).await;
        assert!(history.body.as_array().unwrap().len() >= 3);

        // Terminated for good.
        let ended = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/terminate"),
                Some(&cookie),
                json!({ "on": "2026-10-20" }),
            ),
        )
        .await;
        assert_eq!(ended.status, StatusCode::OK, "{}", ended.body);
        assert_eq!(ended.body["status"], "terminated");
        assert_eq!(ended.body["ends_on"], "2026-10-20");
        let again = send(
            &app,
            json_request(
                "POST",
                &format!("{one}/terminate"),
                Some(&cookie),
                json!({ "on": "2026-10-21" }),
            ),
        )
        .await;
        assert_eq!(again.status, StatusCode::CONFLICT);
        let listed = send(
            &app,
            get_request(&format!("{uri}?status=terminated&q=MENSAL"), Some(&cookie)),
        )
        .await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            send(
                &app,
                get_request(&format!("{uri}?status=gone"), Some(&cookie))
            )
            .await
            .status,
            StatusCode::BAD_REQUEST
        );
        let (other, other_company) = sign_up(&app, "bia@example.com").await;
        let theirs = send(
            &app,
            get_request(
                &format!("{}/{id}", contracts_uri(other_company)),
                Some(&other),
            ),
        )
        .await;
        assert_eq!(theirs.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn contracts_follow_the_sales_role() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let uri = contracts_uri(company_id);
        for (role, may_write) in [
            (Role::Sales, true),
            (Role::Financial, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&uri, Some(&cookie))).await.status,
                StatusCode::OK,
                "{role:?} reads"
            );
            let write = send(&app, json_request("POST", &uri, Some(&cookie), json!({}))).await;
            let expected = if may_write {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(write.status, expected, "{role:?} writes");
        }
    }

    fn commission_rules_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/commission-rules")
    }

    #[tokio::test]
    async fn commission_rules_and_the_statement_of_what_a_seller_earned_through_the_api() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let category = revenue_category(&app, &cookie, company_id).await;
        let seller = send(
            &app,
            json_request(
                "POST",
                &parties_uri(company_id),
                Some(&cookie),
                json!({ "name": "Vera Vendedora", "is_customer": false, "is_supplier": false, "is_seller": true }),
            ),
        )
        .await;
        assert_eq!(seller.status, StatusCode::CREATED, "{}", seller.body);
        let seller = seller.body["id"].as_str().unwrap().to_owned();
        let rules = commission_rules_uri(company_id);

        // The rules: a general rate and a better one for this seller and this product.
        let general = send(
            &app,
            json_request(
                "POST",
                &rules,
                Some(&cookie),
                json!({ "rate": "5", "starts_on": "2026-01-01" }),
            ),
        )
        .await;
        assert_eq!(general.status, StatusCode::CREATED, "{}", general.body);
        assert_eq!(general.body["active"], true);
        let special = send(
            &app,
            json_request(
                "POST",
                &rules,
                Some(&cookie),
                json!({ "seller_id": seller, "product_id": targets.product, "rate": "8", "starts_on": "2026-01-01" }),
            ),
        )
        .await;
        assert_eq!(special.status, StatusCode::CREATED, "{}", special.body);
        for (body, status) in [
            (
                json!({ "rate": "0", "starts_on": "2026-01-01" }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "seller_id": targets.customer, "rate": "5", "starts_on": "2026-01-01" }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "rate": 5, "starts_on": "2026-01-01" }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            assert_eq!(
                send(&app, json_request("POST", &rules, Some(&cookie), body))
                    .await
                    .status,
                status
            );
        }
        let listed = send(&app, get_request(&rules, Some(&cookie))).await;
        assert_eq!(listed.body["items"].as_array().unwrap().len(), 2);
        let mine = send(
            &app,
            get_request(
                &format!("{rules}?seller={seller}&active=true"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(mine.body["items"].as_array().unwrap().len(), 1);
        let general_id = general.body["id"].as_str().unwrap();
        let missing = send(
            &app,
            json_request(
                "PUT",
                &format!("{rules}/{general_id}"),
                Some(&cookie),
                json!({ "rate": "5", "starts_on": "2026-01-01" }),
            ),
        )
        .await;
        assert_eq!(missing.status, StatusCode::UNPROCESSABLE_ENTITY);
        let history = send(
            &app,
            get_request(&format!("{rules}/{general_id}/history"), Some(&cookie)),
        )
        .await;
        assert_eq!(history.body.as_array().unwrap().len(), 1);

        // A sale by the seller, completed: 30 of coffee at 8% and 5 of bread at 5%.
        let mut body = sale_body(&targets, &category);
        body["seller_id"] = json!(seller);
        let sale = send(
            &app,
            json_request("POST", &sales_uri(company_id), Some(&cookie), body),
        )
        .await;
        let sale_id = sale.body["id"].as_str().unwrap().to_owned();
        let order = send(
            &app,
            get_request(
                &format!("/companies/{company_id}/commissions?from=2026-11-01&to=2026-11-30"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(order.status, StatusCode::OK, "{}", order.body);
        assert_eq!(
            order.body["sellers"],
            json!([]),
            "an order earns nothing yet"
        );
        send(
            &app,
            json_request(
                "POST",
                &format!("{}/{sale_id}/complete", sales_uri(company_id)),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        let statement_uri =
            format!("/companies/{company_id}/commissions?from=2026-11-01&to=2026-11-30");
        let statement = send(&app, get_request(&statement_uri, Some(&cookie))).await;
        assert_eq!(statement.body["base"], "35");
        assert_eq!(statement.body["commission"], "2.65");
        assert_eq!(statement.body["sellers"][0]["seller_id"], seller);
        assert_eq!(statement.body["sellers"][0]["sales"][0]["number"], 1);
        assert_eq!(
            statement.body["sellers"][0]["sales"][0]["commission"],
            "2.65"
        );
        // A return comes off the base: one of three coffees refunds 10.
        let returned = send(
            &app,
            json_request(
                "POST",
                &format!("{}/{sale_id}/returns", sales_uri(company_id)),
                Some(&cookie),
                json!({
                    "returned_on": "2026-11-10",
                    "items": [{ "line": 1, "quantity": "1" }],
                    "refund_category_id": category,
                    "refund_due_on": "2026-11-20",
                }),
            ),
        )
        .await;
        assert_eq!(returned.status, StatusCode::CREATED, "{}", returned.body);
        let after = send(&app, get_request(&statement_uri, Some(&cookie))).await;
        assert_eq!(after.body["base"], "25");
        assert_eq!(after.body["commission"], "1.85");
        for bad in [
            "from=2026-11-01",
            "from=x&to=2026-11-30",
            "from=2026-12-01&to=2026-11-01",
        ] {
            let reply = send(
                &app,
                get_request(
                    &format!("/companies/{company_id}/commissions?{bad}"),
                    Some(&cookie),
                ),
            )
            .await;
            assert!(reply.status.is_client_error(), "{bad}: {}", reply.status);
        }
        let (other, other_company) = sign_up(&app, "bia@example.com").await;
        let theirs = send(
            &app,
            get_request(
                &format!("/companies/{other_company}/commissions?from=2026-01-01&to=2026-12-31"),
                Some(&other),
            ),
        )
        .await;
        assert_eq!(theirs.body["sellers"], json!([]));
    }

    #[tokio::test]
    async fn commissions_follow_the_sales_role() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let rules = commission_rules_uri(company_id);
        for (role, may_write) in [
            (Role::Sales, true),
            (Role::Financial, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&rules, Some(&cookie))).await.status,
                StatusCode::OK,
                "{role:?} reads"
            );
            let write = send(&app, json_request("POST", &rules, Some(&cookie), json!({}))).await;
            let expected = if may_write {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(write.status, expected, "{role:?} writes");
        }
    }

    fn price_lists_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/price-lists")
    }

    #[tokio::test]
    async fn price_lists_are_registered_replaced_whole_and_kept_private_through_the_api() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let uri = price_lists_uri(company_id);

        let created = send(
            &app,
            json_request(
                "POST",
                &uri,
                Some(&cookie),
                json!({ "name": " Atacado ", "items": [{ "product_id": targets.product, "price": "8.50" }] }),
            ),
        )
        .await;
        assert_eq!(created.status, StatusCode::CREATED, "{}", created.body);
        assert_eq!(created.body["name"], "Atacado");
        assert_eq!(created.body["active"], true);
        assert_eq!(
            created.body["items"],
            json!([{ "product_id": targets.product, "price": "8.5" }])
        );
        let id = created.body["id"].as_str().unwrap().to_owned();

        for (body, status) in [
            (
                json!({ "name": "Vazia", "items": [] }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "name": "Número", "items": [{ "product_id": targets.product, "price": 8.5 }] }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "name": "Negativa", "items": [{ "product_id": targets.product, "price": "-1" }] }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "name": "Estranho", "items": [{ "product_id": Uuid::new_v4(), "price": "1" }] }),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                json!({ "name": "atacado", "items": [{ "product_id": targets.product, "price": "1" }] }),
                StatusCode::CONFLICT,
            ),
        ] {
            let reply = send(
                &app,
                json_request("POST", &uri, Some(&cookie), body.clone()),
            )
            .await;
            assert_eq!(reply.status, status, "{body}: {}", reply.body);
        }

        // Replacing says whether it is active, or it is refused.
        let one = json!([{ "product_id": targets.product, "price": "9" }]);
        let missing = send(
            &app,
            json_request(
                "PUT",
                &format!("{uri}/{id}"),
                Some(&cookie),
                json!({ "name": "Atacado", "items": one }),
            ),
        )
        .await;
        assert_eq!(missing.status, StatusCode::UNPROCESSABLE_ENTITY);
        let replaced = send(
            &app,
            json_request(
                "PUT",
                &format!("{uri}/{id}"),
                Some(&cookie),
                json!({ "name": "Atacado", "items": one, "active": false }),
            ),
        )
        .await;
        assert_eq!(replaced.status, StatusCode::OK, "{}", replaced.body);
        assert_eq!(replaced.body["items"][0]["price"], "9");
        assert_eq!(replaced.body["active"], false);

        let history = send(
            &app,
            get_request(&format!("{uri}/{id}/history"), Some(&cookie)),
        )
        .await;
        assert_eq!(history.body.as_array().unwrap().len(), 2);
        let active = send(
            &app,
            get_request(&format!("{uri}?active=true"), Some(&cookie)),
        )
        .await;
        assert_eq!(active.body["items"], json!([]));
        let found = send(&app, get_request(&format!("{uri}?q=ATAC"), Some(&cookie))).await;
        assert_eq!(found.body["items"].as_array().unwrap().len(), 1);
        let bad = send(
            &app,
            get_request(&format!("{uri}?active=maybe"), Some(&cookie)),
        )
        .await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);

        // Another company sees nothing of it.
        let (other, other_company) = sign_up(&app, "bia@example.com").await;
        let theirs = price_lists_uri(other_company);
        assert_eq!(
            send(&app, get_request(&theirs, Some(&other))).await.body["items"],
            json!([])
        );
        let stolen = send(
            &app,
            json_request(
                "PUT",
                &format!("{theirs}/{id}"),
                Some(&other),
                json!({ "name": "x", "items": one, "active": true }),
            ),
        )
        .await;
        assert_eq!(stolen.status, StatusCode::NOT_FOUND);
        assert_eq!(
            send(
                &app,
                get_request(&format!("{theirs}/{id}/history"), Some(&other))
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn price_lists_follow_the_sales_role() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        let uri = price_lists_uri(company_id);
        for (role, may_write) in [
            (Role::Sales, true),
            (Role::Financial, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&uri, Some(&cookie))).await.status,
                StatusCode::OK,
                "{role:?} reads"
            );
            let write = send(&app, json_request("POST", &uri, Some(&cookie), json!({}))).await;
            let expected = if may_write {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(write.status, expected, "{role:?} writes");
        }
    }

    #[tokio::test]
    async fn sales_reports_rank_what_was_sold_net_of_returns_with_the_abc_curve() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = sales_targets(&app, &cookie, company_id).await;
        let category = revenue_category(&app, &cookie, company_id).await;
        let uri = format!(
            "/companies/{company_id}/sales-reports?by=product&from=2026-11-01&to=2026-11-30"
        );

        let empty = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(empty.status, StatusCode::OK, "{}", empty.body);
        assert_eq!(empty.body["rows"], json!([]));
        assert_eq!(empty.body["total"], "0");

        // An order counts for nothing; the completed sale counts, less what came back.
        let order = send(
            &app,
            json_request(
                "POST",
                &sales_uri(company_id),
                Some(&cookie),
                sale_body(&targets, &category),
            ),
        )
        .await;
        let order_id = order.body["id"].as_str().unwrap().to_owned();
        let still_order = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(still_order.body["rows"], json!([]));
        send(
            &app,
            json_request(
                "POST",
                &format!("{}/{order_id}/complete", sales_uri(company_id)),
                Some(&cookie),
                Value::Null,
            ),
        )
        .await;
        let report = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(report.body["sales"], 1);
        assert_eq!(report.body["total"], "35");
        let rows = report.body["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["net"], "30");
        assert_eq!(rows[0]["share"], "85.71");
        assert_eq!(rows[0]["class"], "A");
        assert_eq!(rows[1]["net"], "5");
        assert_eq!(rows[1]["cumulative"], "100");
        assert_eq!(rows[1]["class"], "B");

        let returned = send(
            &app,
            json_request(
                "POST",
                &format!("{}/{order_id}/returns", sales_uri(company_id)),
                Some(&cookie),
                json!({
                    "returned_on": "2026-11-10",
                    "items": [{ "line": 1, "quantity": "1" }],
                    "refund_category_id": category,
                    "refund_due_on": "2026-11-20",
                }),
            ),
        )
        .await;
        assert_eq!(returned.status, StatusCode::CREATED, "{}", returned.body);
        let after = send(&app, get_request(&uri, Some(&cookie))).await;
        assert_eq!(after.body["total"], "25");

        let customers = send(
            &app,
            get_request(
                &format!("/companies/{company_id}/sales-reports?by=customer&from=2026-11-01&to=2026-11-30"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(customers.body["rows"][0]["key"], targets.customer);
        assert_eq!(customers.body["rows"][0]["sales"], 1);

        for bad in [
            "by=colour&from=2026-11-01&to=2026-11-30",
            "by=product&from=2026-11-01",
            "by=product&from=x&to=2026-11-30",
            "by=product&from=2026-12-01&to=2026-11-01",
        ] {
            let reply = send(
                &app,
                get_request(
                    &format!("/companies/{company_id}/sales-reports?{bad}"),
                    Some(&cookie),
                ),
            )
            .await;
            assert!(reply.status.is_client_error(), "{bad}: {}", reply.status);
        }
        let (other, other_company) = sign_up(&app, "bia@example.com").await;
        let theirs = send(
            &app,
            get_request(&format!("/companies/{other_company}/sales-reports?by=customer&from=2026-01-01&to=2026-12-31"), Some(&other)),
        )
        .await;
        assert_eq!(theirs.body["rows"], json!([]));
    }

    const STATEMENT: &str = "OFXHEADER:100\nDATA:OFXSGML\nVERSION:102\nCHARSET:1252\n\n<OFX><BANKMSGSRSV1><STMTTRNRS><STMTRS><BANKTRANLIST>\n\
<STMTTRN>\n<TRNTYPE>CREDIT\n<DTPOSTED>20261105120000[-3:BRT]\n<TRNAMT>1250.50\n<FITID>0001\n<MEMO>PIX RECEBIDO Maria\n</STMTTRN>\n\
<STMTTRN>\n<TRNTYPE>DEBIT\n<DTPOSTED>20261106\n<TRNAMT>-30.00\n<FITID>0002\n<NAME>TARIFA\n</STMTTRN>\n\
</BANKTRANLIST></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>";

    fn statement_request(uri: &str, cookie: Option<&str>, body: impl Into<Body>) -> Request<Body> {
        let mut builder = Request::post(uri).header(header::CONTENT_TYPE, "application/x-ofx");
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        builder.body(body.into()).unwrap()
    }

    fn lines_uri(company_id: Uuid) -> String {
        format!("/companies/{company_id}/statement-lines")
    }

    #[tokio::test]
    async fn imports_an_ofx_statement_once_and_reconciles_its_lines_through_the_api() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let account = registered_account(&app, &cookie, company_id, "Caixa").await;
        let import = format!("/companies/{company_id}/bank-accounts/{account}/statement-import");

        // A dry run writes nothing; the file goes in once and sending it again adds nothing.
        let dry = send(
            &app,
            statement_request(&format!("{import}?dry_run=true"), Some(&cookie), STATEMENT),
        )
        .await;
        assert_eq!(dry.status, StatusCode::OK, "{}", dry.body);
        assert_eq!(
            (dry.body["valid"].as_u64(), dry.body["created"].as_u64()),
            (Some(2), Some(0))
        );
        assert_eq!(
            send(&app, get_request(&lines_uri(company_id), Some(&cookie)))
                .await
                .body["items"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        let done = send(&app, statement_request(&import, Some(&cookie), STATEMENT)).await;
        assert_eq!(done.body["created"], 2, "{}", done.body);
        let again = send(&app, statement_request(&import, Some(&cookie), STATEMENT)).await;
        assert_eq!(
            (
                again.body["created"].as_u64(),
                again.body["skipped"].as_array().unwrap().len()
            ),
            (Some(0), 2)
        );
        // The same statement as the Latin-1 a bank may send.
        let latin1 = STATEMENT
            .replace("PIX RECEBIDO Maria", "RECEBIMENTO ação")
            .into_bytes();
        let ok = send(&app, statement_request(&import, Some(&cookie), latin1)).await;
        assert_eq!(ok.status, StatusCode::OK);

        let listed = send(
            &app,
            get_request(
                &format!("{}?status=pending&account={account}", lines_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        let items = listed.body["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["posted_on"], "2026-11-06");
        assert_eq!(items[0]["amount"], "-30");
        assert_eq!(items[0]["description"], "TARIFA");
        assert_eq!(items[0]["status"], "pending");
        let pix = items
            .iter()
            .find(|item| item["description"] == "PIX RECEBIDO Maria")
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let fee = items[0]["id"].as_str().unwrap().to_owned();

        // A sale settled near the line is what is suggested; matching it moves the line.
        let sale = send(
            &app,
            json_request(
                "POST",
                &entries_uri(company_id),
                Some(&cookie),
                entry_body(&targets, "Venda", &[("2026-11-04", "1250.50")]),
            ),
        )
        .await;
        let sale_uri = format!(
            "{}/{}/settlements",
            entries_uri(company_id),
            sale.body["id"].as_str().unwrap()
        );
        let settled = send(
            &app,
            json_request(
                "POST",
                &sale_uri,
                Some(&cookie),
                json!({ "installment": 1, "settled_on": "2026-11-04", "applied": "1250.50", "bank_account_id": account }),
            ),
        )
        .await;
        assert_eq!(settled.status, StatusCode::CREATED, "{}", settled.body);
        let suggestions = send(
            &app,
            get_request(
                &format!("{}/{pix}/suggestions", lines_uri(company_id)),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(suggestions.body[0]["kind"], "settlement");
        assert_eq!(suggestions.body[0]["entry_description"], "Venda");
        assert_eq!(suggestions.body[0]["settlement"]["id"], settled.body["id"]);
        let line = |id: &str, action: &str| format!("{}/{id}/{action}", lines_uri(company_id));
        let matched = send(
            &app,
            json_request(
                "POST",
                &line(&pix, "match"),
                Some(&cookie),
                json!({ "settlement_id": settled.body["id"] }),
            ),
        )
        .await;
        assert_eq!(matched.status, StatusCode::OK, "{}", matched.body);
        assert_eq!(matched.body["status"], "matched");
        assert_eq!(matched.body["matched_settlement_id"], settled.body["id"]);
        let history = send(&app, get_request(&line(&pix, "history"), Some(&cookie))).await;
        assert_eq!(history.body.as_array().unwrap().len(), 2);

        // What does not fit, and what says neither or both, is refused in JSON.
        for body in [
            json!({}),
            json!({ "settlement_id": Uuid::new_v4(), "transfer_id": Uuid::new_v4() }),
        ] {
            let reply = send(
                &app,
                json_request("POST", &line(&fee, "match"), Some(&cookie), body),
            )
            .await;
            assert_eq!(
                reply.status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{}",
                reply.body
            );
        }
        let misfit = send(
            &app,
            json_request(
                "POST",
                &line(&fee, "match"),
                Some(&cookie),
                json!({ "settlement_id": settled.body["id"] }),
            ),
        )
        .await;
        assert_eq!(
            misfit.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            misfit.body
        );
        assert!(
            misfit.body["error"]
                .as_str()
                .unwrap()
                .contains("does not fit")
        );

        // Unmatched, ignored and restored; each answer is the line; a move that is not one is refused.
        let post = |action: &str, id: &str| {
            send(
                &app,
                json_request("POST", &line(id, action), Some(&cookie), Value::Null),
            )
        };
        assert_eq!(post("unmatch", &pix).await.body["status"], "pending");
        assert_eq!(post("ignore", &fee).await.body["status"], "ignored");
        assert_eq!(
            post("ignore", &fee).await.status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(post("restore", &fee).await.body["status"], "pending");

        // An entry made of a line is settled on the day, in the account, and the line is matched to it.
        let made = send(
            &app,
            json_request(
                "POST",
                &line(&fee, "create-entry"),
                Some(&cookie),
                json!({ "category_id": targets.other_category, "description": "Tarifa do banco" }),
            ),
        )
        .await;
        assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
        assert_eq!(made.body["line"]["status"], "matched");
        let entry = send(
            &app,
            get_request(
                &format!(
                    "{}/{}",
                    entries_uri(company_id),
                    made.body["entry_id"].as_str().unwrap()
                ),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(entry.body["kind"], "payable");
        assert_eq!(entry.body["total"], "30");
        assert_eq!(entry.body["installments"][0]["settled"], "30");

        // What is not a statement, not an id, not a status, and not a company's is refused in JSON.
        let not_ofx = send(
            &app,
            statement_request(&import, Some(&cookie), "a;b;c\n1;2;3"),
        )
        .await;
        assert_eq!(
            not_ofx.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            not_ofx.body
        );
        assert!(not_ofx.body["error"].as_str().unwrap().contains("OFX"));
        for query in ["status=done", "account=abc", "after=garbage"] {
            let reply = send(
                &app,
                get_request(&format!("{}?{query}", lines_uri(company_id)), Some(&cookie)),
            )
            .await;
            assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{query}");
        }
        assert_eq!(
            post("ignore", "not-an-id").await.status,
            StatusCode::NOT_FOUND
        );
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        for request in [
            get_request(&lines_uri(company_id), Some(&stranger)),
            statement_request(&import, Some(&stranger), STATEMENT),
            json_request("POST", &line(&pix, "ignore"), Some(&stranger), Value::Null),
        ] {
            assert_eq!(send(&app, request).await.status, StatusCode::NOT_FOUND);
        }
        assert_eq!(
            send(&app, get_request(&lines_uri(company_id), None))
                .await
                .status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn statement_lines_follow_the_finance_role() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        for (role, may_write) in [
            (Role::Admin, true),
            (Role::Financial, true),
            (Role::Sales, false),
            (Role::Stock, false),
            (Role::Accountant, false),
            (Role::Viewer, false),
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            assert_eq!(
                send(&app, get_request(&lines_uri(company_id), Some(&cookie)))
                    .await
                    .status,
                StatusCode::OK,
                "{role:?} reads"
            );
            let import = format!(
                "/companies/{company_id}/bank-accounts/{}/statement-import",
                Uuid::new_v4()
            );
            let write = send(&app, statement_request(&import, Some(&cookie), STATEMENT)).await;
            let expected = if may_write {
                StatusCode::UNPROCESSABLE_ENTITY
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(write.status, expected, "{role:?} imports");
        }
    }

    fn reports_uri(company_id: Uuid, path: &str) -> String {
        format!("/companies/{company_id}/reports/{path}")
    }

    #[tokio::test]
    async fn reports_the_entries_by_group_and_lists_what_is_overdue_through_the_api() {
        let app = entry_app();
        let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
        let targets = entry_targets(&app, &cookie, company_id).await;
        let post = |body: Value| {
            send(
                &app,
                json_request("POST", &entries_uri(company_id), Some(&cookie), body),
            )
        };
        let mut rent = entry_body(&targets, "Aluguel", &[("2026-10-05", "2500")]);
        rent["kind"] = json!("payable");
        rent["category_id"] = json!(targets.other_category);
        rent["party_id"] = Value::Null;
        rent["cost_center_id"] = Value::Null;
        rent["competence_date"] = json!("2026-10-05");
        let rent = post(rent).await;
        let mut sale = entry_body(
            &targets,
            "Venda",
            &[("2026-10-20", "100"), ("2026-11-20", "50.5")],
        );
        sale["competence_date"] = json!("2026-10-10");
        assert_eq!(post(sale).await.status, StatusCode::CREATED);
        assert_eq!(rent.status, StatusCode::CREATED, "{}", rent.body);

        // By category over October, by competence: the biggest first, with the difference of the two totals.
        let by_category = send(
            &app,
            get_request(
                &reports_uri(
                    company_id,
                    "entries?group=category&from=2026-10-01&to=2026-10-31",
                ),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(by_category.status, StatusCode::OK, "{}", by_category.body);
        assert_eq!(
            (
                by_category.body["group"].as_str(),
                by_category.body["basis"].as_str()
            ),
            (Some("category"), Some("competence"))
        );
        assert_eq!(
            by_category.body["rows"],
            json!([
                { "id": targets.other_category, "receivable": "0", "payable": "2500", "net": "-2500", "entries": 1 },
                { "id": targets.category, "receivable": "150.5", "payable": "0", "net": "150.5", "entries": 1 },
            ])
        );
        // By cost center, by due day: what has none is a row of its own, with a null id.
        let by_cost_center = send(
            &app,
            get_request(
                &reports_uri(
                    company_id,
                    "entries?group=cost_center&basis=due&from=2026-10-01&to=2026-10-31",
                ),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(by_cost_center.body["rows"][0]["id"], Value::Null);
        assert_eq!(by_cost_center.body["rows"][1]["id"], targets.cost_center);
        assert_eq!(by_cost_center.body["rows"][1]["receivable"], "100");

        // What is overdue on a day, with its totals and how late it is.
        let overdue = send(
            &app,
            get_request(
                &reports_uri(company_id, "overdue?on=2026-11-15&limit=1"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(overdue.status, StatusCode::OK, "{}", overdue.body);
        assert_eq!(overdue.body["items"].as_array().unwrap().len(), 1);
        assert_eq!(overdue.body["items"][0]["description"], "Aluguel");
        assert_eq!(overdue.body["items"][0]["open"], "2500");
        assert_eq!(overdue.body["items"][0]["days_late"], 41);
        assert_eq!(
            (
                overdue.body["receivable"].as_str(),
                overdue.body["payable"].as_str(),
                overdue.body["count"].as_u64()
            ),
            (Some("100"), Some("2500"), Some(2))
        );
        let receivable = send(
            &app,
            get_request(
                &reports_uri(company_id, "overdue?on=2026-11-15&kind=receivable"),
                Some(&cookie),
            ),
        )
        .await;
        assert_eq!(receivable.body["count"], 1);

        // What is not understood is a 400 in JSON, never ignored.
        for uri in [
            "entries?group=party&from=2026-10-01",
            "entries?from=2026-10-01&to=2026-10-31",
            "entries?group=product&from=2026-10-01&to=2026-10-31",
            "entries?group=party&basis=paid&from=2026-10-01&to=2026-10-31",
            "entries?group=party&from=tomorrow&to=2026-10-31",
            "entries?group=party&from=2026-10-31&to=2026-10-01",
            "overdue",
            "overdue?on=2026-02-30",
            "overdue?on=2026-11-15&kind=sale",
        ] {
            let reply = send(
                &app,
                get_request(&reports_uri(company_id, uri), Some(&cookie)),
            )
            .await;
            assert_eq!(
                reply.status,
                StatusCode::BAD_REQUEST,
                "{uri}: {}",
                reply.body
            );
            assert!(reply.body["error"].is_string(), "{uri}");
        }
        // Another company's member does not learn that it exists.
        let (stranger, _) = sign_up(&app, "bia@example.com").await;
        assert_eq!(
            send(
                &app,
                get_request(
                    &reports_uri(company_id, "overdue?on=2026-11-15"),
                    Some(&stranger)
                )
            )
            .await
            .status,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(
                &app,
                get_request(&reports_uri(company_id, "overdue?on=2026-11-15"), None)
            )
            .await
            .status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn every_role_reads_the_reports() {
        use conta_verde_domain::Role;

        let (app, identities, _) = app_with_mailbox();
        let (_, company_id) = sign_up(&app, "dona@example.com").await;
        for role in [
            Role::Admin,
            Role::Financial,
            Role::Sales,
            Role::Stock,
            Role::Accountant,
            Role::Viewer,
        ] {
            let cookie = member_with_role(&app, &identities, company_id, role).await;
            for path in [
                "entries?group=category&from=2026-10-01&to=2026-10-31",
                "overdue?on=2026-11-15",
            ] {
                assert_eq!(
                    send(
                        &app,
                        get_request(&reports_uri(company_id, path), Some(&cookie))
                    )
                    .await
                    .status,
                    StatusCode::OK,
                    "{role:?} {path}"
                );
            }
        }
    }

    include!("v1/tests.rs");
    include!("v1/webhook_tests.rs");
    include!("bank_connection_tests.rs");
    include!("connector_tests.rs");
}
