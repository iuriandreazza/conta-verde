use std::future::Future;

use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, BankAccountId, BankConnection, BankConnectionId, CalendarDate,
    CompanyId, ConnectionStatus, JobKind, MAX_BANK_CONNECTIONS, NewJob, StatementLine,
    StatementLineDraft, StatementLineId, bank_connection_changes, bank_connection_creation_changes,
    sync_error,
};
use uuid::Uuid;

use crate::audit::audit_entry;
use crate::reconciliation_service::MAX_STATEMENT_LINES;
use crate::{
    AggregatedAccount, AggregatedTransaction, AggregatorError, ApplicationError, BankAggregator,
    BankConnectionRepository, EntryReferences, FinancialEntryRepository, ImportReport, ImportRow,
    JobQueue, JobReport, ReconciliationService, StatementLineRepository, TransferRepository,
};

/// How far back the first pull goes, in days.
const FIRST_PULL_DAYS: i64 = 90;
/// How many days before the last pull each later one starts: a movement posted late is still found, and one found twice adds
/// nothing (the statement line's fingerprint holds the aggregator's id).
const OVERLAP_DAYS: i64 = 7;
const LEASE_SECONDS: i64 = 300;
const DUE_PER_PASS: usize = 200;
const REFUSED_REASON: &str =
    "the provider no longer accepts this connection: connect it again there";

/// What the sync stores statement lines through: the reconciliation use case, as a narrow trait so that the sync does not carry
/// its four type parameters ([ADR 0034]).
pub trait StatementSink: Send + Sync {
    fn can_store_in(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
    ) -> impl Future<Output = Result<bool, ApplicationError>> + Send;

    /// Stores the lines in the account, in the name of `actor`, skipping what it already has.
    fn store(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        account: BankAccountId,
        rows: Vec<ImportRow<StatementLineDraft>>,
    ) -> impl Future<Output = Result<ImportReport, ApplicationError>> + Send;
}

impl<S, E, T, F> StatementSink for ReconciliationService<S, E, T, F>
where
    S: StatementLineRepository,
    E: FinancialEntryRepository,
    T: TransferRepository,
    F: EntryReferences,
{
    async fn can_store_in(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
    ) -> Result<bool, ApplicationError> {
        ReconciliationService::account_is_usable(self, company_id, account).await
    }

    async fn store(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        account: BankAccountId,
        rows: Vec<ImportRow<StatementLineDraft>>,
    ) -> Result<ImportReport, ApplicationError> {
        self.import(company_id, actor, account, rows, false).await
    }
}

/// What a pull did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub created: usize,
    /// Lines the account already had.
    pub skipped: usize,
    /// Movements the aggregator sent that cannot be a statement line (a zero amount, a description that is not text).
    pub refused: usize,
    /// The bank needs the person: nothing was pulled.
    pub needs_attention: bool,
}

/// A company's bank connections, and the pull of their statements into the lines that reconciliation works on ([ADR 0089]). The
/// time is the caller's.
pub struct BankSyncService<C, A, K, Q> {
    connections: C,
    aggregator: A,
    sink: K,
    queue: Q,
}

impl<C, A, K, Q> BankSyncService<C, A, K, Q>
where
    C: BankConnectionRepository,
    A: BankAggregator,
    K: StatementSink,
    Q: JobQueue,
{
    pub fn new(connections: C, aggregator: A, sink: K, queue: Q) -> Self {
        Self {
            connections,
            aggregator,
            sink,
            queue,
        }
    }

    /// Whether an aggregator is configured.
    pub fn is_available(&self) -> bool {
        self.aggregator.is_enabled()
    }

    pub fn provider(&self) -> &'static str {
        self.aggregator.provider()
    }

    /// A token for the aggregator's widget, which the person uses to connect their bank.
    pub async fn connect_token(
        &self,
        company_id: CompanyId,
        item: Option<&str>,
    ) -> Result<String, ApplicationError> {
        self.ensure_enabled()?;
        self.aggregator
            .connect_token(company_id, item)
            .await
            .map_err(failure)
    }

    /// The accounts inside a connection the person made, to choose which one feeds a bank account.
    pub async fn accounts_of(
        &self,
        item: &str,
    ) -> Result<Vec<AggregatedAccount>, ApplicationError> {
        self.ensure_enabled()?;
        self.aggregator.accounts(item).await.map_err(failure)
    }

    /// Links a bank account of the company to an account at the aggregator, and queues the first pull.
    pub async fn connect(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        bank_account_id: BankAccountId,
        item_id: &str,
        account_id: &str,
        now_millis: i64,
    ) -> Result<BankConnection, ApplicationError> {
        self.ensure_enabled()?;
        if self.connections.list(company_id).await?.len() >= MAX_BANK_CONNECTIONS {
            return Err(conta_verde_domain::DomainError::TooManyBankConnections.into());
        }
        if !self.sink.can_store_in(company_id, bank_account_id).await? {
            return Err(ApplicationError::UnusableReference("account"));
        }
        let connection = BankConnection::new(
            BankConnectionId::generate(),
            company_id,
            bank_account_id,
            self.aggregator.provider(),
            item_id,
            account_id,
            actor.clone(),
            now_millis,
        )?;
        // The account must be one the connection holds: an id that is not its own is the person's mistake, said now.
        let accounts = self
            .aggregator
            .accounts(connection.item_id())
            .await
            .map_err(failure)?;
        if !accounts
            .iter()
            .any(|account| account.id == connection.account_id())
        {
            return Err(ApplicationError::UnusableReference("aggregator account"));
        }
        let audit = audit_entry(
            &connection,
            actor,
            AuditAction::Created,
            bank_connection_creation_changes(&connection),
        );
        self.connections.save(&connection, &audit).await?;
        self.enqueue(&connection, now_millis.div_euclid(1000))
            .await?;
        Ok(connection)
    }

    /// Ends a connection: it is not pulled again, and the aggregator is told (a failure to tell it is logged, not the person's
    /// problem: the connection is ended here either way).
    pub async fn disconnect(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: BankConnectionId,
    ) -> Result<BankConnection, ApplicationError> {
        let before = self.find(company_id, id).await?;
        if before.status() == ConnectionStatus::Disconnected {
            return Ok(before);
        }
        let after = before.with_status(ConnectionStatus::Disconnected, None);
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            bank_connection_changes(&before, &after),
        );
        self.connections
            .update_status(&after, &audit)
            .await
            .map_err(missing)?;
        // The item is the person's whole consent with the bank, and other accounts of it may still feed a bank account: it is ended
        // at the aggregator only when nothing else uses it.
        let in_use = self
            .connections
            .item_is_live(before.provider(), before.item_id())
            .await?;
        if !in_use
            && self.aggregator.is_enabled()
            && let Err(error) = self.aggregator.disconnect(before.item_id()).await
        {
            tracing::warn!(%error, "the aggregator was not told of a disconnected bank connection");
        }
        Ok(after)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
    ) -> Result<Vec<BankConnection>, ApplicationError> {
        Ok(self.connections.list(company_id).await?)
    }

    pub async fn history(
        &self,
        company_id: CompanyId,
        id: BankConnectionId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's connection answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.connections.history(company_id, id).await?)
    }

    /// Pulls the statement of a connection through `today` and stores it as statement lines, in the name of whoever connected it.
    /// A connection the bank needs the person for is marked so and pulls nothing; one the aggregator could not answer for fails,
    /// so that the queue tries again, and says why on the connection.
    pub async fn sync(
        &self,
        company_id: CompanyId,
        id: BankConnectionId,
        today: CalendarDate,
    ) -> Result<SyncReport, ApplicationError> {
        self.ensure_enabled()?;
        let connection = self.find(company_id, id).await?;
        self.pull(connection, today).await
    }

    /// Queues a pull for each connection that has not pulled through `today`, of every company. Asking again is harmless: a
    /// pull that is repeated stores nothing twice.
    pub async fn enqueue_due(
        &self,
        today: CalendarDate,
        now_seconds: i64,
    ) -> Result<usize, ApplicationError> {
        let due = self.connections.due(today, DUE_PER_PASS).await?;
        for connection in &due {
            self.enqueue(connection, now_seconds).await?;
        }
        Ok(due.len())
    }

    /// Does the pulls that are due at `now_seconds`, at most `limit`. A pull that fails is tried again as the kind says.
    pub async fn run_due(
        &self,
        now_seconds: i64,
        today: CalendarDate,
        limit: usize,
    ) -> Result<JobReport, ApplicationError> {
        let mut report = JobReport::default();
        for _ in 0..limit {
            let Some(job) = self
                .queue
                .claim_next_of(now_seconds, LEASE_SECONDS, &[JobKind::SyncBankConnection])
                .await?
            else {
                break;
            };
            let id = job
                .args
                .first()
                .and_then(|text| Uuid::parse_str(text).ok())
                .map(BankConnectionId::from_uuid);
            let outcome = match id {
                Some(id) if job.attempts <= job.kind.max_attempts() => {
                    self.run_one(id, today).await
                }
                Some(_) => Err("gave up after repeated crashes".to_owned()),
                None => Ok(()),
            };
            match outcome {
                Ok(()) => {
                    self.queue.complete(job.id).await?;
                    report.done += 1;
                }
                Err(reason) => {
                    let retry_at = job.retry_after().map(|wait| now_seconds + wait);
                    if retry_at.is_none() {
                        report.dead += 1;
                    } else {
                        report.retried += 1;
                    }
                    self.queue.fail(job.id, &reason, retry_at).await?;
                }
            }
        }
        Ok(report)
    }

    async fn run_one(&self, id: BankConnectionId, today: CalendarDate) -> Result<(), String> {
        let Some(connection) = self
            .connections
            .find_by_id(id)
            .await
            .map_err(|_| "storage error".to_owned())?
        else {
            return Ok(());
        };
        // Ended, or waiting for the person, while the job waited: nothing to pull.
        if connection.status() != ConnectionStatus::Connected {
            return Ok(());
        }
        match self.pull(connection, today).await {
            Ok(_) => Ok(()),
            Err(error) => {
                tracing::error!(%error, "a bank connection could not be pulled");
                Err(error.to_string())
            }
        }
    }

    async fn pull(
        &self,
        connection: BankConnection,
        today: CalendarDate,
    ) -> Result<SyncReport, ApplicationError> {
        if connection.status() == ConnectionStatus::Disconnected {
            return Err(conta_verde_domain::DomainError::InvalidBankConnection("status").into());
        }
        let state = match self.aggregator.item_state(connection.item_id()).await {
            Ok(state) => state,
            Err(error) => return self.failed(&connection, error).await,
        };
        if let crate::ItemState::NeedsAttention(reason) = state {
            if connection.status() != ConnectionStatus::NeedsAttention {
                self.move_to(&connection, ConnectionStatus::NeedsAttention, Some(&reason))
                    .await?;
            }
            return Ok(SyncReport {
                created: 0,
                skipped: 0,
                refused: 0,
                needs_attention: true,
            });
        }
        // The person did what the bank asked: only asking again can bring the connection back, since the hourly pass leaves the
        // ones waiting for them alone.
        let connection = if connection.status() == ConnectionStatus::NeedsAttention {
            self.move_to(&connection, ConnectionStatus::Connected, None)
                .await?
        } else {
            connection
        };
        let from = window_start(&connection, today);
        let transactions = match self
            .aggregator
            .transactions(connection.account_id(), from, today)
            .await
        {
            Ok(transactions) => transactions,
            Err(error) => return self.failed(&connection, error).await,
        };
        let (mut pending, refused) = self.lines_of(&connection, transactions);
        // One import takes a bounded number of rows; the day is recorded only after all of them are stored, so a pull that fails
        // half way is made again whole.
        let mut report = ImportReport::default();
        while !pending.is_empty() {
            let rest = pending.split_off(pending.len().min(MAX_STATEMENT_LINES));
            let stored = self
                .sink
                .store(
                    connection.company_id(),
                    connection.connected_by(),
                    connection.bank_account_id(),
                    pending,
                )
                .await?;
            report.valid += stored.valid;
            report.created += stored.created;
            report.skipped.extend(stored.skipped);
            pending = rest;
        }
        self.connections
            .record_sync(&connection.synced_through(today))
            .await?;
        Ok(SyncReport {
            created: report.created,
            skipped: report.skipped.len(),
            refused,
            needs_attention: false,
        })
    }

    /// The lines a pull stores, and how many movements could not be one: a movement that cannot be a line is left out and
    /// counted, because the import takes none of a file when one is refused and one odd movement must not stop a company's
    /// statement for ever.
    fn lines_of(
        &self,
        connection: &BankConnection,
        transactions: Vec<AggregatedTransaction>,
    ) -> (Vec<ImportRow<StatementLineDraft>>, usize) {
        let mut rows = Vec::new();
        let mut refused = 0;
        for transaction in transactions {
            let draft = StatementLineDraft {
                posted_on: transaction.posted_on.to_string(),
                amount: transaction.amount,
                description: transaction.description,
                bank_ref: Some(transaction.id),
            };
            let probe = StatementLine::new(
                StatementLineId::generate(),
                connection.company_id(),
                connection.bank_account_id(),
                draft.clone(),
                0,
            );
            if probe.is_ok() {
                rows.push(ImportRow {
                    row: rows.len() + 1,
                    draft,
                });
            } else {
                refused += 1;
            }
        }
        (rows, refused)
    }

    /// An aggregator that could not answer: the connection says why, and the pull fails so that the queue tries it again.
    async fn failed(
        &self,
        connection: &BankConnection,
        error: AggregatorError,
    ) -> Result<SyncReport, ApplicationError> {
        // The aggregator no longer accepts this connection (it was removed there, the consent was revoked): no retry changes
        // that, so it waits for the person like a bank that asks for them, instead of being queued every hour for ever.
        if let AggregatorError::Refused(_) = &error {
            tracing::warn!(%error, "a bank aggregator refused a connection");
            if connection.status() != ConnectionStatus::NeedsAttention {
                self.move_to(
                    connection,
                    ConnectionStatus::NeedsAttention,
                    Some(REFUSED_REASON),
                )
                .await?;
            }
            return Ok(SyncReport {
                created: 0,
                skipped: 0,
                refused: 0,
                needs_attention: true,
            });
        }
        let reason = sync_error(&error.to_string());
        let mut noted = connection.clone();
        noted = noted.with_status(noted.status(), Some(&reason));
        if let Err(storage) = self.connections.record_sync(&noted).await {
            tracing::error!(error = %storage, "why a bank pull failed could not be written");
        }
        Err(failure(error))
    }

    /// Moves a connection to `status`, in the name of whoever connected it, and says why.
    async fn move_to(
        &self,
        connection: &BankConnection,
        status: ConnectionStatus,
        reason: Option<&str>,
    ) -> Result<BankConnection, ApplicationError> {
        let after = connection.with_status(status, reason);
        let audit = audit_entry(
            &after,
            connection.connected_by(),
            AuditAction::Updated,
            bank_connection_changes(connection, &after),
        );
        self.connections
            .update_status(&after, &audit)
            .await
            .map_err(missing)?;
        Ok(after)
    }

    async fn enqueue(
        &self,
        connection: &BankConnection,
        now_seconds: i64,
    ) -> Result<(), ApplicationError> {
        let job = NewJob::new(
            JobKind::SyncBankConnection,
            vec![connection.id().as_uuid().to_string()],
            now_seconds,
        )?;
        self.queue.enqueue(&job).await?;
        Ok(())
    }

    fn ensure_enabled(&self) -> Result<(), ApplicationError> {
        if self.aggregator.is_enabled() {
            Ok(())
        } else {
            Err(ApplicationError::AggregatorOff)
        }
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: BankConnectionId,
    ) -> Result<BankConnection, ApplicationError> {
        self.connections
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::BankConnectionNotFound)
    }
}

/// The first day a pull asks for: a few days before the last one reached, or three months back the first time.
fn window_start(connection: &BankConnection, today: CalendarDate) -> CalendarDate {
    let days = match connection.last_synced_on() {
        Some(last) => last.to_unix_days() - OVERLAP_DAYS,
        None => today.to_unix_days() - FIRST_PULL_DAYS,
    };
    CalendarDate::from_unix_days(days).unwrap_or(CalendarDate::EARLIEST)
}

fn failure(error: AggregatorError) -> ApplicationError {
    match error {
        AggregatorError::Disabled => ApplicationError::AggregatorOff,
        AggregatorError::Unavailable(_) | AggregatorError::Refused(_) => {
            tracing::warn!(%error, "a bank aggregator call failed");
            ApplicationError::AggregatorUnavailable
        }
    }
}

fn missing(error: crate::RepositoryError) -> ApplicationError {
    match error {
        // It vanished between the read and the write.
        crate::RepositoryError::NotFound => ApplicationError::BankConnectionNotFound,
        other => other.into(),
    }
}
