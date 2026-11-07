use std::time::Duration;

use conta_verde_api::{ApiConfig, Ports, Services, router};
use conta_verde_application::{
    AttachmentService, AuthService, BalanceService, BankAccountService, CategoryService,
    CommissionService, ContractService, CostCenterService, DocumentMailService,
    FinancialEntryService, InventoryCountService, JobRunner, MemberService, PartyService,
    PaymentMethodService, PaymentTermService, PostalCodeService, PriceListService, ProductService,
    PurchaseService, QuoteService, ReconciliationService, RecurrenceService, SaleService,
    SalesReportService, StockService, TransferService,
};
use conta_verde_application::{
    ChargeService, CollectionService, DunningService, EmailError, EmailMessage, EmailSender,
    MessageError, MessageSender, PaymentPageService, PaymentProvider, PaymentWebhookService,
    ProviderCharge, ProviderChargeRequest, ProviderError, ProviderEvent, QueuedEmailSender,
    TextMessage, WebhookError, WebhookHeaders,
};
use conta_verde_infrastructure::{
    Argon2SecretService, BrasilApiCnpjRegistry, BrasilApiPostalCodeDirectory, CachedCnpjRegistry,
    LocalBlobStore, LogEmailSender, PostgresAttachmentRepository, PostgresBankAccountRepository,
    PostgresCategoryRepository, PostgresCommissionRuleRepository, PostgresContractRepository,
    PostgresCostCenterRepository, PostgresDocumentMailRepository, PostgresFinancialEntryRepository,
    PostgresIdentityRepository, PostgresInventoryCountRepository, PostgresInvitationRepository,
    PostgresJobQueue, PostgresOpeningRepository, PostgresPartyRepository,
    PostgresPaymentMethodRepository, PostgresPaymentTermRepository, PostgresPriceListRepository,
    PostgresProductRepository, PostgresPurchaseOrderRepository, PostgresQuoteRepository,
    PostgresRecurrenceRepository, PostgresSaleRepository, PostgresStatementLineRepository,
    PostgresStockMovementRepository, PostgresStockSettingsRepository, PostgresTransferRepository,
    RegisterDocumentReferences, RegisterReferences, SmtpEmailSender, connect_and_migrate,
};
use conta_verde_infrastructure::{
    AsaasProvider, NoMessageSender, PostgresDunningPlanRepository, PostgresNotificationRepository,
    StripeProvider, WhatsAppCloudSender,
};
use conta_verde_infrastructure::{
    FakePaymentProvider, NoPaymentProvider, PostgresChargeRepository,
    PostgresCollectionSettingsRepository, PostgresPaymentEventRepository,
    PostgresPaymentLinkRepository,
};
use tracing_subscriber::EnvFilter;

/// The adapters of a running installation.
struct PostgresPorts;

impl Ports for PostgresPorts {
    type Parties = PostgresPartyRepository;
    type Registry = Option<CachedCnpjRegistry<BrasilApiCnpjRegistry>>;
    type Identity = PostgresIdentityRepository;
    type Secrets = Argon2SecretService;
    type Products = PostgresProductRepository;
    type CostCenters = PostgresCostCenterRepository;
    type Categories = PostgresCategoryRepository;
    type BankAccounts = PostgresBankAccountRepository;
    type PaymentMethods = PostgresPaymentMethodRepository;
    type PaymentTerms = PostgresPaymentTermRepository;
    type PriceLists = PostgresPriceListRepository;
    type Entries = PostgresFinancialEntryRepository;
    type EntryReferences = RegisterReferences<
        PostgresCategoryRepository,
        PostgresCostCenterRepository,
        PostgresPartyRepository,
        PostgresBankAccountRepository,
        PostgresPaymentMethodRepository,
    >;
    type Attachments = PostgresAttachmentRepository;
    type Blobs = LocalBlobStore;
    type Charges = PostgresChargeRepository;
    type Provider = ConfiguredPaymentProvider;
    type CollectionSettings = PostgresCollectionSettingsRepository;
    type PaymentEvents = PostgresPaymentEventRepository;
    type PaymentLinks = PostgresPaymentLinkRepository;
    type DunningPlans = PostgresDunningPlanRepository;
    type Notifications = PostgresNotificationRepository;
    type Messages = ConfiguredMessageSender;
    type Transfers = PostgresTransferRepository;
    type Openings = PostgresOpeningRepository;
    type Recurrences = PostgresRecurrenceRepository;
    type DocumentReferences = RegisterDocumentReferences<
        PostgresPartyRepository,
        PostgresProductRepository,
        PostgresPaymentTermRepository,
    >;
    type Quotes = PostgresQuoteRepository;
    type Sales = PostgresSaleRepository;
    type Purchases = PostgresPurchaseOrderRepository;
    type StockMovements = PostgresStockMovementRepository;
    type StockSettings = PostgresStockSettingsRepository;
    type Counts = PostgresInventoryCountRepository;
    type Contracts = PostgresContractRepository;
    type CommissionRules = PostgresCommissionRuleRepository;
    type Statements = PostgresStatementLineRepository;
    type PostalCodes = Option<BrasilApiPostalCodeDirectory>;
    type Invitations = PostgresInvitationRepository;
    type Emails = QueuedEmailSender<PostgresJobQueue>;
    type DocumentMails = PostgresDocumentMailRepository;
}

const SESSION_LIFETIME: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const INVITATION_LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const JOB_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// How often the worker looks for jobs that are due: a message waits at most this long.
const JOBS_POLL_INTERVAL: Duration = Duration::from_secs(5);
const JOBS_PER_PASS: usize = 50;
const CNPJ_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let database_url = std::env::var("DATABASE_URL")?;
    // Loopback by default: put a TLS-terminating reverse proxy in front before exposing the API.
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
    let config = ApiConfig {
        cookie_secure: parse_flag("COOKIE_SECURE")?,
    };

    let pool = connect_and_migrate(&database_url).await?;
    let parties = PartyService::new(
        PostgresPartyRepository::new(pool.clone()),
        cnpj_registry_from_env()?,
    );
    let products = ProductService::new(PostgresProductRepository::new(pool.clone()));
    let cost_centers = CostCenterService::new(PostgresCostCenterRepository::new(pool.clone()));
    let categories = CategoryService::new(PostgresCategoryRepository::new(pool.clone()));
    let bank_accounts = BankAccountService::new(PostgresBankAccountRepository::new(pool.clone()));
    let payment_methods =
        PaymentMethodService::new(PostgresPaymentMethodRepository::new(pool.clone()));
    let payment_terms = PaymentTermService::new(PostgresPaymentTermRepository::new(pool.clone()));
    let references = || {
        RegisterReferences::new(
            PostgresCategoryRepository::new(pool.clone()),
            PostgresCostCenterRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            PostgresBankAccountRepository::new(pool.clone()),
            PostgresPaymentMethodRepository::new(pool.clone()),
        )
    };
    let entries = FinancialEntryService::new(
        PostgresFinancialEntryRepository::new(pool.clone()),
        references(),
    );
    let attachments = AttachmentService::new(
        PostgresAttachmentRepository::new(pool.clone()),
        LocalBlobStore::new(attachments_dir_from_env()),
        PostgresFinancialEntryRepository::new(pool.clone()),
    );
    // Links in e-mails (an invitation, a quote, a payment) lead to the web app, which is where the person
    // answers.
    let public_url = std::env::var("PUBLIC_URL").unwrap_or_else(|_| {
        tracing::warn!(
            "PUBLIC_URL is not set: invitation links will lead to http://localhost:5173"
        );
        "http://localhost:5173".to_owned()
    });
    let payment_provider = payment_provider_from_env(&public_url)?;
    let charges = ChargeService::new(
        PostgresChargeRepository::new(pool.clone()),
        PostgresFinancialEntryRepository::new(pool.clone()),
        PostgresPartyRepository::new(pool.clone()),
        payment_provider.clone(),
        PostgresCollectionSettingsRepository::new(pool.clone()),
    );
    let webhooks = PaymentWebhookService::new(
        payment_provider,
        PostgresChargeRepository::new(pool.clone()),
        PostgresPaymentEventRepository::new(pool.clone()),
        PostgresCollectionSettingsRepository::new(pool.clone()),
        FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(),
        ),
    );
    let collection = CollectionService::new(
        PostgresCollectionSettingsRepository::new(pool.clone()),
        references(),
    );
    let transfers =
        TransferService::new(PostgresTransferRepository::new(pool.clone()), references());
    let recurrence_service = || {
        RecurrenceService::new(
            PostgresRecurrenceRepository::new(pool.clone()),
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(),
            ),
            references(),
        )
    };
    let recurrences = recurrence_service();
    tokio::spawn(generate_recurring_entries(recurrence_service()));
    let reconciliation = ReconciliationService::new(
        PostgresStatementLineRepository::new(pool.clone()),
        FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(),
        ),
        TransferService::new(PostgresTransferRepository::new(pool.clone()), references()),
        references(),
    );
    let balances = BalanceService::new(
        PostgresBankAccountRepository::new(pool.clone()),
        PostgresFinancialEntryRepository::new(pool.clone()),
        PostgresTransferRepository::new(pool.clone()),
        PostgresOpeningRepository::new(pool.clone()),
    );
    let postal_codes = PostalCodeService::new(postal_code_directory_from_env()?);
    let quotes = QuoteService::new(
        PostgresQuoteRepository::new(pool.clone()),
        RegisterDocumentReferences::new(
            PostgresPartyRepository::new(pool.clone()),
            PostgresProductRepository::new(pool.clone()),
            PostgresPaymentTermRepository::new(pool.clone()),
        ),
        queued_emails(PostgresJobQueue::new(pool.clone()))?,
        Argon2SecretService::new()?,
        &public_url,
    );
    let payment_page_service = || {
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(PaymentPageService::new(
            PostgresPaymentLinkRepository::new(pool.clone()),
            PostgresChargeRepository::new(pool.clone()),
            PostgresFinancialEntryRepository::new(pool.clone()),
            Argon2SecretService::new()?,
            &public_url,
        ))
    };
    let payment_pages = payment_page_service()?;
    let dunning_service = || {
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(DunningService::new(
            PostgresDunningPlanRepository::new(pool.clone()),
            PostgresFinancialEntryRepository::new(pool.clone()),
            PostgresPartyRepository::new(pool.clone()),
            payment_page_service()?,
            PostgresNotificationRepository::new(pool.clone()),
            PostgresIdentityRepository::new(pool.clone()),
            queued_emails(PostgresJobQueue::new(pool.clone()))?,
            message_sender_from_env()?,
        ))
    };
    let dunning = dunning_service()?;
    tokio::spawn(send_reminders(dunning_service()?));
    let price_lists = PriceListService::new(
        PostgresPriceListRepository::new(pool.clone()),
        RegisterDocumentReferences::new(
            PostgresPartyRepository::new(pool.clone()),
            PostgresProductRepository::new(pool.clone()),
            PostgresPaymentTermRepository::new(pool.clone()),
        ),
    );
    let sales_reports = SalesReportService::new(PostgresSaleRepository::new(pool.clone()));
    let stock_service = || {
        StockService::new(
            PostgresStockMovementRepository::new(pool.clone()),
            PostgresProductRepository::new(pool.clone()),
            PostgresStockSettingsRepository::new(pool.clone()),
        )
    };
    let sale_service = || {
        SaleService::new(
            PostgresSaleRepository::new(pool.clone()),
            PostgresQuoteRepository::new(pool.clone()),
            FinancialEntryService::new(
                PostgresFinancialEntryRepository::new(pool.clone()),
                references(),
            ),
            references(),
            RegisterDocumentReferences::new(
                PostgresPartyRepository::new(pool.clone()),
                PostgresProductRepository::new(pool.clone()),
                PostgresPaymentTermRepository::new(pool.clone()),
            ),
            stock_service(),
        )
    };
    let contract_service = || {
        ContractService::new(
            PostgresContractRepository::new(pool.clone()),
            sale_service(),
        )
    };
    let commissions = CommissionService::new(
        PostgresCommissionRuleRepository::new(pool.clone()),
        PostgresSaleRepository::new(pool.clone()),
        RegisterDocumentReferences::new(
            PostgresPartyRepository::new(pool.clone()),
            PostgresProductRepository::new(pool.clone()),
            PostgresPaymentTermRepository::new(pool.clone()),
        ),
    );
    let purchases = PurchaseService::new(
        PostgresPurchaseOrderRepository::new(pool.clone()),
        FinancialEntryService::new(
            PostgresFinancialEntryRepository::new(pool.clone()),
            references(),
        ),
        references(),
        RegisterDocumentReferences::new(
            PostgresPartyRepository::new(pool.clone()),
            PostgresProductRepository::new(pool.clone()),
            PostgresPaymentTermRepository::new(pool.clone()),
        ),
        stock_service(),
    );
    let stock = stock_service();
    let counts = InventoryCountService::new(
        PostgresInventoryCountRepository::new(pool.clone()),
        stock_service(),
    );
    let sales = sale_service();
    let contracts = contract_service();
    tokio::spawn(bill_contracts(contract_service()));
    let members = MemberService::new(
        PostgresInvitationRepository::new(pool.clone()),
        PostgresIdentityRepository::new(pool.clone()),
        queued_emails(PostgresJobQueue::new(pool.clone()))?,
        Argon2SecretService::new()?,
        INVITATION_LIFETIME,
        &public_url,
    );
    tokio::spawn(run_jobs(JobRunner::new(
        PostgresJobQueue::new(pool.clone()),
        email_sender_from_env()?,
    )));
    let documents = DocumentMailService::new(
        PostgresPurchaseOrderRepository::new(pool.clone()),
        PostgresFinancialEntryRepository::new(pool.clone()),
        PostgresPartyRepository::new(pool.clone()),
        queued_emails(PostgresJobQueue::new(pool.clone()))?,
        PostgresDocumentMailRepository::new(pool.clone()),
    );
    let auth = AuthService::new(
        PostgresIdentityRepository::new(pool),
        Argon2SecretService::new()?,
        SESSION_LIFETIME,
    );
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(%bind_addr, "listening");
    axum::serve(
        listener,
        router::<PostgresPorts>(
            Services {
                parties,
                products,
                cost_centers,
                categories,
                bank_accounts,
                payment_methods,
                payment_terms,
                price_lists,
                sales_reports,
                entries,
                attachments,
                charges,
                webhooks,
                payment_pages,
                dunning,
                collection,
                transfers,
                recurrences,
                quotes,
                sales,
                purchases,
                stock,
                counts,
                contracts,
                commissions,
                reconciliation,
                balances,
                postal_codes,
                members,
                documents,
                auth,
            },
            config,
        ),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

/// Where attachments are kept on disk (ADR 0036): `ATTACHMENTS_DIR`, or `./data/attachments`. It holds
/// the only copy of what people attach, so it belongs in the installation's backups.
fn attachments_dir_from_env() -> String {
    std::env::var("ATTACHMENTS_DIR").unwrap_or_else(|_| {
        tracing::warn!("ATTACHMENTS_DIR is not set: attachments are kept in ./data/attachments");
        "./data/attachments".to_owned()
    })
}

/// Every hour, sends the reminders the plans of the companies call for ([ADR 0048]): the day is the one the
/// customers are in, and each reminder is claimed in the store first, so a pass that overlaps another, or a
/// restart, sends nothing twice.
async fn send_reminders<D, E, P, K, N, I, M, W>(service: DunningService<D, E, P, K, N, I, M, W>)
where
    D: conta_verde_application::DunningPlanRepository,
    E: conta_verde_application::FinancialEntryRepository,
    P: conta_verde_application::PartyRepository,
    K: conta_verde_application::InstallmentLinks,
    N: conta_verde_application::NotificationRepository,
    I: conta_verde_application::IdentityRepository,
    M: EmailSender,
    W: MessageSender,
{
    let mut tick = tokio::time::interval(JOB_INTERVAL);
    loop {
        tick.tick().await;
        match service.send_due(conta_verde_api::customer_today()).await {
            Ok(report) if report.sent > 0 || report.failed > 0 => {
                tracing::info!(sent = report.sent, failed = report.failed, "reminders sent");
            }
            Ok(_) => {}
            Err(error) => tracing::error!(%error, "sending reminders failed"),
        }
    }
}

/// The scheduled job of ADR 0033: once an hour, generates the recurring entries that fall due in the
/// next two months. It is safe to repeat and to run in several instances, so a failed pass only waits for
/// the next one.
async fn generate_recurring_entries<R, E, F>(service: RecurrenceService<R, E, F>)
where
    R: conta_verde_application::RecurrenceRepository,
    E: conta_verde_application::FinancialEntryRepository,
    F: conta_verde_application::EntryReferences,
{
    let mut tick = tokio::time::interval(JOB_INTERVAL);
    loop {
        tick.tick().await;
        match service.generate_all_due(conta_verde_api::today()).await {
            Ok(report) if report.made > 0 || report.failed > 0 => {
                tracing::info!(
                    made = report.made,
                    failed = report.failed,
                    "recurring entries generated"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::error!(%error, "generating recurring entries failed"),
        }
    }
}

/// The scheduled job of ADR 0040: once an hour, bills the cycles of the contracts that have fallen due. It is
/// safe to repeat and to run in several instances, so a failed pass only waits for the next one.
async fn bill_contracts<C, S, Q, E, F, R, T>(service: ContractService<C, S, Q, E, F, R, T>)
where
    C: conta_verde_application::ContractRepository,
    S: conta_verde_application::SaleRepository,
    Q: conta_verde_application::QuoteRepository,
    E: conta_verde_application::FinancialEntryRepository,
    F: conta_verde_application::EntryReferences,
    R: conta_verde_application::DocumentReferences,
    T: conta_verde_application::DocumentStock,
{
    let mut tick = tokio::time::interval(JOB_INTERVAL);
    loop {
        tick.tick().await;
        match service
            .bill_all_due(conta_verde_api::customer_today())
            .await
        {
            Ok(report) if report.billed > 0 || report.failed > 0 => {
                tracing::info!(
                    billed = report.billed,
                    failed = report.failed,
                    "contracts billed"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::error!(%error, "billing contracts failed"),
        }
    }
}

/// The payment provider the operator chose with `PAYMENT_PROVIDER`: `fake` for development, and off when it
/// is not set (ADR 0044).
#[derive(Clone)]
enum ConfiguredPaymentProvider {
    Off(NoPaymentProvider),
    Fake(FakePaymentProvider),
    Asaas(Box<AsaasProvider>),
    Stripe(Box<StripeProvider>),
}

impl PaymentProvider for ConfiguredPaymentProvider {
    fn key(&self) -> &str {
        match self {
            Self::Off(provider) => provider.key(),
            Self::Fake(provider) => provider.key(),
            Self::Asaas(provider) => provider.key(),
            Self::Stripe(provider) => provider.key(),
        }
    }

    fn is_enabled(&self) -> bool {
        match self {
            Self::Off(provider) => provider.is_enabled(),
            Self::Fake(provider) => provider.is_enabled(),
            Self::Asaas(provider) => provider.is_enabled(),
            Self::Stripe(provider) => provider.is_enabled(),
        }
    }

    async fn create_charge(
        &self,
        request: &ProviderChargeRequest,
    ) -> Result<ProviderCharge, ProviderError> {
        match self {
            Self::Off(provider) => provider.create_charge(request).await,
            Self::Fake(provider) => provider.create_charge(request).await,
            Self::Asaas(provider) => provider.create_charge(request).await,
            Self::Stripe(provider) => provider.create_charge(request).await,
        }
    }

    async fn cancel_charge(&self, external_id: &str) -> Result<(), ProviderError> {
        match self {
            Self::Off(provider) => provider.cancel_charge(external_id).await,
            Self::Fake(provider) => provider.cancel_charge(external_id).await,
            Self::Asaas(provider) => provider.cancel_charge(external_id).await,
            Self::Stripe(provider) => provider.cancel_charge(external_id).await,
        }
    }

    fn verify_webhook(
        &self,
        headers: &WebhookHeaders,
        body: &[u8],
    ) -> Result<Vec<ProviderEvent>, WebhookError> {
        match self {
            Self::Off(provider) => provider.verify_webhook(headers, body),
            Self::Fake(provider) => provider.verify_webhook(headers, body),
            Self::Asaas(provider) => provider.verify_webhook(headers, body),
            Self::Stripe(provider) => provider.verify_webhook(headers, body),
        }
    }
}

fn payment_provider_from_env(
    public_url: &str,
) -> Result<ConfiguredPaymentProvider, Box<dyn std::error::Error + Send + Sync>> {
    let setting = std::env::var("PAYMENT_PROVIDER").ok();
    match setting.as_deref().map(str::trim) {
        None | Some("" | "off") => {
            tracing::warn!("PAYMENT_PROVIDER is not set: charges are switched off");
            Ok(ConfiguredPaymentProvider::Off(NoPaymentProvider))
        }
        Some("fake") => {
            tracing::warn!("PAYMENT_PROVIDER=fake: charges are made up and cannot be paid");
            // Its webhooks are signed with a secret of the operator's, or with a random one nobody knows.
            let secret = std::env::var("FAKE_PAYMENT_WEBHOOK_SECRET")
                .ok()
                .filter(|secret| !secret.trim().is_empty())
                .unwrap_or_else(|| {
                    tracing::warn!(
                        "FAKE_PAYMENT_WEBHOOK_SECRET is not set: no webhook can be signed"
                    );
                    uuid::Uuid::new_v4().to_string()
                });
            Ok(ConfiguredPaymentProvider::Fake(
                FakePaymentProvider::with_webhook_secret(secret),
            ))
        }
        Some("asaas") => {
            let setting = |name: &str| {
                std::env::var(name)
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            };
            let (Some(key), Some(token)) =
                (setting("ASAAS_API_KEY"), setting("ASAAS_WEBHOOK_TOKEN"))
            else {
                return Err(
                    "PAYMENT_PROVIDER=asaas needs ASAAS_API_KEY and ASAAS_WEBHOOK_TOKEN".into(),
                );
            };
            // Asaas itself refuses a shorter one when the webhook is registered; a short one here would be
            // the only thing between the internet and a forged payment.
            if token.chars().count() < 32 {
                return Err("ASAAS_WEBHOOK_TOKEN must have at least 32 characters".into());
            }
            tracing::info!("charges are made at Asaas");
            Ok(ConfiguredPaymentProvider::Asaas(Box::new(
                AsaasProvider::new(setting("ASAAS_API_URL").as_deref(), &key, &token)?,
            )))
        }
        Some("stripe") => {
            let setting = |name: &str| {
                std::env::var(name)
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            };
            let (Some(key), Some(secret)) = (
                setting("STRIPE_SECRET_KEY"),
                setting("STRIPE_WEBHOOK_SECRET"),
            ) else {
                return Err(
                    "PAYMENT_PROVIDER=stripe needs STRIPE_SECRET_KEY and STRIPE_WEBHOOK_SECRET"
                        .into(),
                );
            };
            // The signing secret of a Stripe webhook endpoint starts with whsec_; anything else is the wrong
            // value (a key, say) and would never verify a notice.
            if !secret.starts_with("whsec_") {
                return Err(
                    "STRIPE_WEBHOOK_SECRET must be the endpoint's signing secret (whsec_...)"
                        .into(),
                );
            }
            tracing::info!("charges are made at Stripe");
            Ok(ConfiguredPaymentProvider::Stripe(Box::new(
                StripeProvider::new(
                    setting("STRIPE_API_URL").as_deref(),
                    &key,
                    &secret,
                    public_url,
                )?,
            )))
        }
        Some(other) => Err(format!("PAYMENT_PROVIDER={other} is not a known provider").into()),
    }
}

/// The WhatsApp provider the operator chose: the Cloud API when its three settings are given, and none
/// otherwise, in which case reminders by WhatsApp are not tried.
enum ConfiguredMessageSender {
    None(NoMessageSender),
    // Boxed: the client is large next to the unit of the off adapter.
    WhatsApp(Box<WhatsAppCloudSender>),
}

impl MessageSender for ConfiguredMessageSender {
    fn is_enabled(&self) -> bool {
        match self {
            Self::None(sender) => sender.is_enabled(),
            Self::WhatsApp(sender) => sender.is_enabled(),
        }
    }

    async fn send(&self, message: &TextMessage) -> Result<(), MessageError> {
        match self {
            Self::None(sender) => sender.send(message).await,
            Self::WhatsApp(sender) => sender.send(message).await,
        }
    }
}

/// `WHATSAPP_PHONE_NUMBER_ID`, `WHATSAPP_TOKEN` and `WHATSAPP_TEMPLATE` together send reminders through the
/// WhatsApp Business Cloud API; with none of them WhatsApp is off. Some without the others is a mistake, and
/// stops the server instead of silently sending nothing. The token is never logged.
fn message_sender_from_env()
-> Result<ConfiguredMessageSender, Box<dyn std::error::Error + Send + Sync>> {
    let setting = |name: &str| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    };
    match (
        setting("WHATSAPP_PHONE_NUMBER_ID"),
        setting("WHATSAPP_TOKEN"),
        setting("WHATSAPP_TEMPLATE"),
    ) {
        (Some(phone), Some(token), Some(template)) => Ok(ConfiguredMessageSender::WhatsApp(
            Box::new(WhatsAppCloudSender::new(&phone, &token, &template)?),
        )),
        (None, None, None) => Ok(ConfiguredMessageSender::None(NoMessageSender)),
        _ => Err(
            "WHATSAPP_PHONE_NUMBER_ID, WHATSAPP_TOKEN and WHATSAPP_TEMPLATE must be set together"
                .into(),
        ),
    }
}

/// The mail sender the operator chose: an SMTP server when `SMTP_URL` is set, and otherwise the log.
enum ConfiguredEmailSender {
    // Boxed: the transport is large next to the unit of the log adapter.
    Smtp(Box<SmtpEmailSender>),
    Log(LogEmailSender),
}

impl EmailSender for ConfiguredEmailSender {
    fn is_enabled(&self) -> bool {
        matches!(self, Self::Smtp(_))
    }

    async fn send(&self, message: &EmailMessage) -> Result<(), EmailError> {
        match self {
            Self::Smtp(sender) => sender.send(message).await,
            Self::Log(sender) => sender.send(message).await,
        }
    }
}

/// What asks for an e-mail (an invitation, a quote) hands it to the queue ([ADR 0059]); the worker sends it. With no
/// mail server configured nothing is queued, and the page shows the link to pass on.
fn queued_emails(
    queue: PostgresJobQueue,
) -> Result<QueuedEmailSender<PostgresJobQueue>, Box<dyn std::error::Error + Send + Sync>> {
    Ok(QueuedEmailSender::new(
        queue,
        email_sender_from_env()?.is_enabled(),
    ))
}

/// The worker of ADR 0059: every few seconds, does the jobs that are due. Safe to run in several instances, since a
/// job is taken by one worker at a time and one a dead worker held is taken again when its lease ends.
async fn run_jobs<Q, E>(runner: JobRunner<Q, E>)
where
    Q: conta_verde_application::JobQueue,
    E: EmailSender,
{
    let mut tick = tokio::time::interval(JOBS_POLL_INTERVAL);
    loop {
        tick.tick().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| {
                i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
            });
        match runner.run_due(now, JOBS_PER_PASS).await {
            Ok(report) if report.done > 0 || report.retried > 0 || report.dead > 0 => {
                tracing::info!(
                    done = report.done,
                    retried = report.retried,
                    dead = report.dead,
                    "jobs run"
                );
            }
            Ok(_) => {}
            Err(error) => tracing::error!(%error, "running jobs failed"),
        }
    }
}

/// `SMTP_URL` and `SMTP_FROM` together send real e-mail; with neither, messages go to the log and
/// nothing is sent (the web app then shows an invitation's link to pass on). One without the other
/// is a mistake, and stops the server instead of silently not sending.
fn email_sender_from_env() -> Result<ConfiguredEmailSender, Box<dyn std::error::Error + Send + Sync>>
{
    // `SMTP_URL=` in an env file or a compose file means "none", not "an empty server".
    let setting = |name: &str| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    };
    email_sender(
        setting("SMTP_URL").as_deref(),
        setting("SMTP_FROM").as_deref(),
    )
}

fn email_sender(
    url: Option<&str>,
    from: Option<&str>,
) -> Result<ConfiguredEmailSender, Box<dyn std::error::Error + Send + Sync>> {
    match (url, from) {
        (Some(url), Some(from)) if !url.is_empty() => {
            tracing::info!("e-mail is sent through the SMTP server of SMTP_URL");
            Ok(ConfiguredEmailSender::Smtp(Box::new(SmtpEmailSender::new(
                url, from,
            )?)))
        }
        (None, None) => {
            tracing::warn!("SMTP_URL is not set: e-mail is only logged, nothing is sent");
            Ok(ConfiguredEmailSender::Log(LogEmailSender))
        }
        _ => Err("SMTP_URL and SMTP_FROM must be set together".into()),
    }
}

/// `None` when the lookup is switched off. A company the registry already answered for is
/// remembered for an hour, which keeps retries and repeated suppliers off the shared rate limit.
fn cnpj_registry_from_env() -> Result<
    Option<CachedCnpjRegistry<BrasilApiCnpjRegistry>>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let setting = std::env::var("CNPJ_LOOKUP").ok();
    if !lookup_enabled("CNPJ_LOOKUP", setting.as_deref())? {
        tracing::warn!("CNPJ lookup is off: CNPJs are validated by check digits only");
        return Ok(None);
    }
    tracing::info!("CNPJs are checked against BrasilAPI (Receita Federal open data)");
    Ok(Some(CachedCnpjRegistry::new(
        BrasilApiCnpjRegistry::new()?,
        CNPJ_CACHE_TTL,
    )))
}

/// `None` when the lookup is switched off. Only a postal code is sent to the third party.
fn postal_code_directory_from_env()
-> Result<Option<BrasilApiPostalCodeDirectory>, Box<dyn std::error::Error + Send + Sync>> {
    let setting = std::env::var("CEP_LOOKUP").ok();
    if !lookup_enabled("CEP_LOOKUP", setting.as_deref())? {
        tracing::warn!("CEP lookup is off: addresses are filled in by hand");
        return Ok(None);
    }
    tracing::info!("postal codes are looked up on BrasilAPI (Correios and other public sources)");
    Ok(Some(BrasilApiPostalCodeDirectory::new()?))
}

// `off` is for installations that must not send a CNPJ or a postal code to a third party, or run
// without internet. Strict like `parse_flag`, so a typo is not read as "off".
fn lookup_enabled(name: &str, setting: Option<&str>) -> Result<bool, String> {
    match setting {
        None | Some("brasilapi") => Ok(true),
        Some("off") => Ok(false),
        Some(other) => Err(format!(
            "{name} must be `brasilapi` or `off`, got `{other}`"
        )),
    }
}

// Strict on purpose: a typo such as `TRUE` must not silently turn off a security setting.
fn parse_flag(name: &str) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    match std::env::var(name).as_deref() {
        Err(_) | Ok("false") => Ok(false),
        Ok("true") => Ok(true),
        Ok(other) => Err(format!("{name} must be `true` or `false`, got `{other}`").into()),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl-c");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lookup_is_on_by_default_and_off_only_when_asked() {
        assert_eq!(lookup_enabled("CNPJ_LOOKUP", None), Ok(true));
        assert_eq!(lookup_enabled("CNPJ_LOOKUP", Some("brasilapi")), Ok(true));
        assert_eq!(lookup_enabled("CNPJ_LOOKUP", Some("off")), Ok(false));
    }

    #[test]
    fn e_mail_goes_to_the_log_without_a_server_and_through_smtp_with_one() {
        const FROM: &str = "conta-verde <no-reply@erp.example>";
        assert!(matches!(
            email_sender(None, None),
            Ok(ConfiguredEmailSender::Log(_))
        ));
        assert!(matches!(
            email_sender(Some("smtp://localhost:1025"), Some(FROM)),
            Ok(ConfiguredEmailSender::Smtp(_))
        ));
    }

    #[test]
    fn half_a_mail_setting_stops_the_server_instead_of_silently_not_sending() {
        const FROM: &str = "conta-verde <no-reply@erp.example>";
        for (url, from) in [
            (Some("smtp://localhost:1025"), None),
            (None, Some(FROM)),
            (Some("not a url"), Some(FROM)),
            (Some("smtp://localhost:1025"), Some("not an address")),
        ] {
            assert!(email_sender(url, from).is_err(), "{url:?} {from:?}");
        }
    }

    #[test]
    fn a_typo_is_an_error_not_a_silent_off() {
        for typo in ["OFF", "false", "", "brasil-api"] {
            let error = lookup_enabled("CEP_LOOKUP", Some(typo)).unwrap_err();
            // The message names the setting that was wrong.
            assert!(error.starts_with("CEP_LOOKUP must be"), "{typo}");
        }
    }
}
