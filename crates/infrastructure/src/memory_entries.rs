use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use conta_verde_application::{
    AccountMovement, CompetenceTotal, DayMovement, DayOpen, EntryCursor, EntrySort,
    FinancialEntryQuery, FinancialEntryRepository, OverdueItem, OverdueReport, ReportBasis,
    ReportGroup, ReportRow, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, BankAccountId, CalendarDate, CategoryId, CompanyId,
    CostCenterId, Decimal, EntryId, EntryKind, EntryOrigin, FinancialEntry, Settlement,
    SettlementId, settled_of,
};

use crate::memory_audit::AuditLog;

/// Keeps entries and their audit trail in process memory. Used by tests and for running the API
/// without a database. Like the Postgres adapter, it records an audit entry only for a write that
/// happened: a refused or missed write leaves nothing in the trail, and the installments of an entry
/// it could not change stay as they were.
/// Clones share the same entries, so that a test can hand one handle to the use case that writes and
/// another to the one that adds balances up.
#[derive(Default, Clone)]
pub struct InMemoryFinancialEntryRepository {
    state: Arc<Mutex<EntryState>>,
}

#[derive(Default)]
struct EntryState {
    entries: Vec<FinancialEntry>,
    settlements: Vec<Settlement>,
    audit: AuditLog,
}

impl FinancialEntryRepository for InMemoryFinancialEntryRepository {
    async fn save(
        &self,
        entry: &FinancialEntry,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, entry)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        if entry.origin().is_some_and(|origin| {
            state
                .entries
                .iter()
                .any(|stored| stored.origin() == Some(origin))
        }) {
            return Err(RepositoryError::DuplicateOrigin);
        }
        state.entries.push(entry.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> Result<Option<FinancialEntry>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .entries
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.id() == id)
            .cloned())
    }

    async fn update(
        &self,
        entry: &FinancialEntry,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, entry)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .entries
            .iter()
            .position(|stored| {
                stored.company_id() == entry.company_id() && stored.id() == entry.id()
            })
            .ok_or(RepositoryError::NotFound)?;
        let stored = &state.entries[position];
        if stored.is_voided() {
            return Err(RepositoryError::EntryVoided);
        }
        let settled = state
            .settlements
            .iter()
            .any(|settlement| settlement.entry_id() == entry.id() && !settlement.is_reversed());
        if settled
            && (stored.kind() != entry.kind()
                || stored.installments_text() != entry.installments_text())
        {
            return Err(RepositoryError::EntrySettled);
        }
        state.entries[position] = entry.clone();
        state.audit.record(audit);
        Ok(())
    }

    async fn settle(
        &self,
        settlement: &Settlement,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, settlement)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .entries
            .iter()
            .find(|stored| {
                stored.company_id() == settlement.company_id()
                    && stored.id() == settlement.entry_id()
            })
            .ok_or(RepositoryError::NotFound)?;
        if stored.is_voided() {
            return Err(RepositoryError::EntryVoided);
        }
        let installment = stored
            .installment(settlement.installment())
            .ok_or(RepositoryError::NotFound)?;
        // A charge settles once, even if that settlement was reversed since.
        if settlement.charge_id().is_some()
            && state
                .settlements
                .iter()
                .any(|other| other.charge_id() == settlement.charge_id())
        {
            return Err(RepositoryError::DuplicateOrigin);
        }
        let settled = settled_of(
            &state
                .settlements
                .iter()
                .filter(|other| other.entry_id() == settlement.entry_id())
                .cloned()
                .collect::<Vec<_>>(),
            settlement.installment(),
        );
        if settled + settlement.applied().as_decimal() > installment.amount().as_decimal() {
            return Err(RepositoryError::Overpaid);
        }
        state.settlements.push(settlement.clone());
        state.audit.record(audit);
        Ok(())
    }

    async fn reverse(
        &self,
        reversed: &Settlement,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, reversed)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let stored = state
            .settlements
            .iter_mut()
            .find(|stored| {
                stored.company_id() == reversed.company_id()
                    && stored.entry_id() == reversed.entry_id()
                    && stored.id() == reversed.id()
                    && !stored.is_reversed()
            })
            .ok_or(RepositoryError::NotFound)?;
        *stored = stored.clone().with_reversed(true);
        state.audit.record(audit);
        Ok(())
    }

    async fn settlement(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: SettlementId,
    ) -> Result<Option<Settlement>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .settlements
            .iter()
            .find(|stored| {
                stored.company_id() == company_id
                    && stored.entry_id() == entry_id
                    && stored.id() == id
            })
            .cloned())
    }

    async fn find_many(
        &self,
        company_id: CompanyId,
        ids: &[EntryId],
    ) -> Result<Vec<FinancialEntry>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<FinancialEntry> = state
            .entries
            .iter()
            .filter(|stored| stored.company_id() == company_id && ids.contains(&stored.id()))
            .cloned()
            .collect();
        found.sort_by_key(|entry| (entry.competence_date(), entry.id().as_uuid()));
        Ok(found)
    }

    async fn settlements_between(
        &self,
        company_id: CompanyId,
        since: CalendarDate,
        until: CalendarDate,
        limit: usize,
    ) -> Result<Vec<Settlement>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<Settlement> = state
            .settlements
            .iter()
            .filter(|stored| {
                stored.company_id() == company_id
                    && !stored.is_reversed()
                    && (since..=until).contains(&stored.settled_on())
            })
            .cloned()
            .collect();
        found.sort_by_key(|stored| (stored.settled_on(), stored.id().as_uuid()));
        found.truncate(limit);
        Ok(found)
    }

    async fn settlements_of(
        &self,
        company_id: CompanyId,
        entry_ids: &[EntryId],
    ) -> Result<Vec<Settlement>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<Settlement> = state
            .settlements
            .iter()
            .filter(|stored| {
                stored.company_id() == company_id && entry_ids.contains(&stored.entry_id())
            })
            .cloned()
            .collect();
        found.sort_by_key(|stored| (stored.settled_on(), stored.id().as_uuid()));
        Ok(found)
    }

    async fn cash_of_account(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        since: Option<CalendarDate>,
        until: CalendarDate,
    ) -> Result<AccountMovement, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut movement = AccountMovement::default();
        for settlement in state.settlements.iter().filter(|stored| {
            stored.company_id() == company_id
                && stored.bank_account_id() == account
                && !stored.is_reversed()
                && stored.settled_on() <= until
                && since.is_none_or(|day| stored.settled_on() >= day)
        }) {
            let kind = state
                .entries
                .iter()
                .find(|entry| {
                    entry.company_id() == company_id && entry.id() == settlement.entry_id()
                })
                .map(FinancialEntry::kind);
            let total: &mut Decimal = match kind {
                Some(EntryKind::Receivable) => &mut movement.received,
                Some(EntryKind::Payable) => &mut movement.paid,
                None => continue,
            };
            *total += settlement.cash();
        }
        movement.received = movement.received.normalize();
        movement.paid = movement.paid.normalize();
        Ok(movement)
    }

    async fn settlement_candidates(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        kind: EntryKind,
        cash: Decimal,
        since: CalendarDate,
        until: CalendarDate,
    ) -> Result<Vec<Settlement>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<Settlement> = state
            .settlements
            .iter()
            .filter(|settlement| {
                settlement.company_id() == company_id
                    && settlement.bank_account_id() == account
                    && !settlement.is_reversed()
                    && settlement.cash() == cash.normalize()
                    && (since..=until).contains(&settlement.settled_on())
                    && state.entries.iter().any(|entry| {
                        entry.company_id() == company_id
                            && entry.id() == settlement.entry_id()
                            && entry.kind() == kind
                    })
            })
            .cloned()
            .collect();
        found.sort_by_key(|settlement| (settlement.settled_on(), settlement.id().as_uuid()));
        Ok(found)
    }

    async fn report_entries(
        &self,
        company_id: CompanyId,
        group: ReportGroup,
        basis: ReportBasis,
        since: CalendarDate,
        until: CalendarDate,
    ) -> Result<Vec<ReportRow>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut groups: BTreeMap<Option<uuid::Uuid>, (Decimal, Decimal, u64)> = BTreeMap::new();
        for entry in state
            .entries
            .iter()
            .filter(|entry| entry.company_id() == company_id && !entry.is_voided())
        {
            let amount = match basis {
                ReportBasis::Competence => (since..=until)
                    .contains(&entry.competence_date())
                    .then(|| entry.total()),
                ReportBasis::Due => {
                    let due: Decimal = entry
                        .installments()
                        .iter()
                        .filter(|installment| (since..=until).contains(&installment.due_date()))
                        .map(|installment| installment.amount().as_decimal())
                        .sum();
                    entry
                        .installments()
                        .iter()
                        .any(|installment| (since..=until).contains(&installment.due_date()))
                        .then_some(due)
                }
            };
            let Some(amount) = amount else { continue };
            let key = match group {
                ReportGroup::Category => Some(entry.category_id().as_uuid()),
                ReportGroup::CostCenter => entry.cost_center_id().map(|id| id.as_uuid()),
                ReportGroup::Party => entry.party_id().map(|id| id.as_uuid()),
            };
            let row = groups.entry(key).or_default();
            match entry.kind() {
                EntryKind::Receivable => row.0 += amount,
                EntryKind::Payable => row.1 += amount,
            }
            row.2 += 1;
        }
        let mut rows: Vec<ReportRow> = groups
            .into_iter()
            .map(|(id, (receivable, payable, entries))| ReportRow {
                id,
                receivable: receivable.normalize(),
                payable: payable.normalize(),
                entries,
            })
            .collect();
        crate::postgres_entries::sort_report(&mut rows);
        Ok(rows)
    }

    async fn competence_totals(
        &self,
        company_id: CompanyId,
        since: CalendarDate,
        until: CalendarDate,
    ) -> Result<Vec<CompetenceTotal>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        // Ordered by the same key as the Postgres query: month, category, then cost center with none first.
        let mut totals: BTreeMap<
            (CalendarDate, uuid::Uuid, Option<uuid::Uuid>),
            (Decimal, Decimal),
        > = BTreeMap::new();
        for entry in state.entries.iter().filter(|entry| {
            entry.company_id() == company_id
                && !entry.is_voided()
                && (since..=until).contains(&entry.competence_date())
        }) {
            let key = (
                entry.competence_date().first_of_month(),
                entry.category_id().as_uuid(),
                entry.cost_center_id().map(|id| id.as_uuid()),
            );
            let total = totals.entry(key).or_default();
            match entry.kind() {
                EntryKind::Receivable => total.0 += entry.total(),
                EntryKind::Payable => total.1 += entry.total(),
            }
        }
        Ok(totals
            .into_iter()
            .map(
                |((month, category, cost_center), (receivable, payable))| CompetenceTotal {
                    month,
                    category_id: CategoryId::from_uuid(category),
                    cost_center_id: cost_center.map(CostCenterId::from_uuid),
                    receivable: receivable.normalize(),
                    payable: payable.normalize(),
                },
            )
            .collect())
    }

    async fn receivable_installments_due(
        &self,
        company_id: CompanyId,
        since: CalendarDate,
        until: CalendarDate,
    ) -> Result<Vec<OverdueItem>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut found: Vec<OverdueItem> = Vec::new();
        for entry in state.entries.iter().filter(|entry| {
            entry.company_id() == company_id
                && !entry.is_voided()
                && entry.kind() == EntryKind::Receivable
        }) {
            let settlements: Vec<Settlement> = state
                .settlements
                .iter()
                .filter(|settlement| settlement.entry_id() == entry.id())
                .cloned()
                .collect();
            for installment in entry.installments() {
                let settled = settled_of(&settlements, installment.number());
                if installment.due_date() >= since
                    && installment.due_date() <= until
                    && installment.amount().as_decimal() > settled
                {
                    found.push(OverdueItem {
                        entry_id: entry.id(),
                        kind: entry.kind(),
                        description: entry.description().to_owned(),
                        party_id: entry.party_id(),
                        category_id: entry.category_id(),
                        installment: installment.number(),
                        due_date: installment.due_date(),
                        amount: installment.amount().as_decimal().normalize(),
                        settled: settled.normalize(),
                    });
                }
            }
        }
        found.sort_by_key(|item| (item.due_date, item.entry_id.as_uuid(), item.installment));
        Ok(found)
    }

    async fn overdue_installments(
        &self,
        company_id: CompanyId,
        before: CalendarDate,
        kind: Option<EntryKind>,
        limit: usize,
    ) -> Result<OverdueReport, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut all: Vec<OverdueItem> = Vec::new();
        for entry in state.entries.iter().filter(|entry| {
            entry.company_id() == company_id
                && !entry.is_voided()
                && kind.is_none_or(|kind| entry.kind() == kind)
        }) {
            let settlements: Vec<Settlement> = state
                .settlements
                .iter()
                .filter(|settlement| settlement.entry_id() == entry.id())
                .cloned()
                .collect();
            for installment in entry.installments() {
                let settled = settled_of(&settlements, installment.number());
                if installment.due_date() < before && installment.amount().as_decimal() > settled {
                    all.push(OverdueItem {
                        entry_id: entry.id(),
                        kind: entry.kind(),
                        description: entry.description().to_owned(),
                        party_id: entry.party_id(),
                        category_id: entry.category_id(),
                        installment: installment.number(),
                        due_date: installment.due_date(),
                        amount: installment.amount().as_decimal().normalize(),
                        settled: settled.normalize(),
                    });
                }
            }
        }
        all.sort_by_key(|item| (item.due_date, item.entry_id.as_uuid(), item.installment));
        let open = |item: &OverdueItem| item.amount - item.settled;
        let total = |wanted: EntryKind| {
            all.iter()
                .filter(|item| item.kind == wanted)
                .map(open)
                .sum::<Decimal>()
                .normalize()
        };
        let report = OverdueReport {
            receivable: total(EntryKind::Receivable),
            payable: total(EntryKind::Payable),
            count: all.len() as u64,
            items: all.into_iter().take(limit).collect(),
        };
        Ok(report)
    }

    async fn cash_by_day(
        &self,
        company_id: CompanyId,
        account: BankAccountId,
        since: CalendarDate,
        until: CalendarDate,
    ) -> Result<Vec<DayMovement>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut days: BTreeMap<CalendarDate, AccountMovement> = BTreeMap::new();
        for settlement in state.settlements.iter().filter(|stored| {
            stored.company_id() == company_id
                && stored.bank_account_id() == account
                && !stored.is_reversed()
                && (since..=until).contains(&stored.settled_on())
        }) {
            let Some(kind) = state
                .entries
                .iter()
                .find(|entry| {
                    entry.company_id() == company_id && entry.id() == settlement.entry_id()
                })
                .map(FinancialEntry::kind)
            else {
                continue;
            };
            let day = days.entry(settlement.settled_on()).or_default();
            match kind {
                EntryKind::Receivable => day.received += settlement.cash(),
                EntryKind::Payable => day.paid += settlement.cash(),
            }
        }
        Ok(days
            .into_iter()
            .map(|(date, moved)| DayMovement {
                date,
                received: moved.received.normalize(),
                paid: moved.paid.normalize(),
            })
            .collect())
    }

    async fn open_by_due_day(
        &self,
        company_id: CompanyId,
        since: CalendarDate,
        until: CalendarDate,
    ) -> Result<Vec<DayOpen>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let mut days: BTreeMap<CalendarDate, (Decimal, Decimal)> = BTreeMap::new();
        for entry in state
            .entries
            .iter()
            .filter(|entry| entry.company_id() == company_id && !entry.is_voided())
        {
            let settlements: Vec<Settlement> = state
                .settlements
                .iter()
                .filter(|settlement| settlement.entry_id() == entry.id())
                .cloned()
                .collect();
            for installment in entry.installments() {
                let left = installment.amount().as_decimal()
                    - settled_of(&settlements, installment.number());
                if left <= Decimal::ZERO || !(since..=until).contains(&installment.due_date()) {
                    continue;
                }
                let day = days.entry(installment.due_date()).or_default();
                match entry.kind() {
                    EntryKind::Receivable => day.0 += left,
                    EntryKind::Payable => day.1 += left,
                }
            }
        }
        Ok(days
            .into_iter()
            .map(|(date, (receivable, payable))| DayOpen {
                date,
                receivable: receivable.normalize(),
                payable: payable.normalize(),
            })
            .collect())
    }

    async fn void(
        &self,
        voided: &FinancialEntry,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, voided)?;
        let mut state = self.state.lock().expect("repository lock poisoned");
        let position = state
            .entries
            .iter()
            .position(|stored| {
                stored.company_id() == voided.company_id()
                    && stored.id() == voided.id()
                    && !stored.is_voided()
            })
            .ok_or(RepositoryError::NotFound)?;
        if state
            .settlements
            .iter()
            .any(|settlement| settlement.entry_id() == voided.id() && !settlement.is_reversed())
        {
            return Err(RepositoryError::EntrySettled);
        }
        state.entries[position] = state.entries[position]
            .voided()
            .map_err(|_| RepositoryError::NotFound)?;
        state.audit.record(audit);
        Ok(())
    }

    async fn find_by_origin(
        &self,
        company_id: CompanyId,
        origin: EntryOrigin,
    ) -> Result<Option<FinancialEntry>, RepositoryError> {
        if matches!(origin, EntryOrigin::Recurrence(_)) {
            return Err(RepositoryError::Storage(
                "an entry is looked up by sale or return, never by occurrence".to_owned(),
            ));
        }
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .entries
            .iter()
            .find(|stored| stored.company_id() == company_id && stored.origin() == Some(origin))
            .cloned())
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        Ok(state
            .audit
            .history(company_id, AuditEntity::FinancialEntry, id.as_uuid()))
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &FinancialEntryQuery,
        after: Option<&EntryCursor>,
        limit: usize,
    ) -> Result<Vec<FinancialEntry>, RepositoryError> {
        let state = self.state.lock().expect("repository lock poisoned");
        let descending = query.sort() == EntrySort::CompetenceDesc;
        let key = |entry: &FinancialEntry| (entry.competence_date(), entry.id().as_uuid());
        let mut rows: Vec<FinancialEntry> = state
            .entries
            .iter()
            .filter(|stored| stored.company_id() == company_id && query.matches(stored))
            .filter(|stored| {
                after.is_none_or(|cursor| {
                    let after_key = (cursor.date, cursor.id);
                    if descending {
                        key(stored) < after_key
                    } else {
                        key(stored) > after_key
                    }
                })
            })
            .cloned()
            .collect();
        rows.sort_by_key(key);
        if descending {
            rows.reverse();
        }
        rows.truncate(limit);
        Ok(rows)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use conta_verde_application::FinancialEntryService;
    use conta_verde_domain::CompanyId;

    use super::*;
    use crate::contract::{self, EntrySeed};
    use crate::{
        InMemoryBankAccountRepository, InMemoryCategoryRepository, InMemoryCostCenterRepository,
        InMemoryPartyRepository, InMemoryPaymentMethodRepository, InMemoryPeriodLockRepository,
        RegisterReferences,
    };

    /// The registers an entry points at, shared by handle with the references that read them.
    pub(crate) struct Registers {
        pub(crate) categories: InMemoryCategoryRepository,
        pub(crate) cost_centers: InMemoryCostCenterRepository,
        pub(crate) parties: InMemoryPartyRepository,
        pub(crate) bank_accounts: InMemoryBankAccountRepository,
        pub(crate) payment_methods: InMemoryPaymentMethodRepository,
        /// The closed period every service of a test built from these registers sees.
        pub(crate) period_locks: InMemoryPeriodLockRepository,
    }

    impl Registers {
        pub(crate) fn new() -> Self {
            Self {
                categories: InMemoryCategoryRepository::default(),
                cost_centers: InMemoryCostCenterRepository::default(),
                parties: InMemoryPartyRepository::default(),
                bank_accounts: InMemoryBankAccountRepository::default(),
                payment_methods: InMemoryPaymentMethodRepository::default(),
                period_locks: InMemoryPeriodLockRepository::default(),
            }
        }

        pub(crate) async fn seed(&self, company: CompanyId) -> EntrySeed {
            contract::seed_entry_records(
                &self.categories,
                &self.cost_centers,
                &self.parties,
                &self.bank_accounts,
                &self.payment_methods,
                company,
            )
            .await
        }

        pub(crate) fn references(
            &self,
        ) -> RegisterReferences<
            InMemoryCategoryRepository,
            InMemoryCostCenterRepository,
            InMemoryPartyRepository,
            InMemoryBankAccountRepository,
            InMemoryPaymentMethodRepository,
            InMemoryPeriodLockRepository,
        > {
            RegisterReferences::new(
                self.categories.clone(),
                self.cost_centers.clone(),
                self.parties.clone(),
                self.bank_accounts.clone(),
                self.payment_methods.clone(),
                self.period_locks.clone(),
            )
        }
    }

    #[tokio::test]
    async fn the_registers_say_what_an_entry_may_point_at() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        contract::entry_references_say_what_is_usable(
            &registers.references(),
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    async fn stores_entries_with_their_trail() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        contract::stores_entries_with_their_trail(
            &InMemoryFinancialEntryRepository::default(),
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    async fn edits_entries_and_records_only_changes() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let service = FinancialEntryService::new(
            InMemoryFinancialEntryRepository::default(),
            registers.references(),
        );
        contract::edits_entries_and_records_only_changes(&service, company, other_company, &seed)
            .await;
    }

    #[tokio::test]
    async fn an_edit_may_keep_what_was_deactivated_since() {
        let registers = Registers::new();
        let company = CompanyId::generate();
        let seed = registers.seed(company).await;
        let service = FinancialEntryService::new(
            InMemoryFinancialEntryRepository::default(),
            registers.references(),
        );
        contract::an_edit_may_keep_what_was_deactivated_since(
            &service,
            &registers.categories,
            company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    async fn settles_installments_and_reverses_them() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let service = FinancialEntryService::new(
            InMemoryFinancialEntryRepository::default(),
            registers.references(),
        );
        contract::settles_installments_and_reverses_them(&service, company, other_company, &seed)
            .await;
    }

    #[tokio::test]
    async fn imports_entries_from_a_file() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let service = FinancialEntryService::new(
            InMemoryFinancialEntryRepository::default(),
            registers.references(),
        );
        contract::imports_entries_from_a_file(
            &service,
            &registers.categories,
            company,
            other_company,
            &seed,
        )
        .await;
    }

    #[tokio::test]
    async fn reports_delinquency() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let entries = FinancialEntryService::new(
            InMemoryFinancialEntryRepository::default(),
            registers.references(),
        );
        contract::reports_delinquency(&entries, &registers.parties, company, other_company, &seed)
            .await;
    }

    #[tokio::test]
    async fn reports_the_income_statement() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let (seed, other_seed) = (
            registers.seed(company).await,
            registers.seed(other_company).await,
        );
        let store = InMemoryFinancialEntryRepository::default();
        let entries = FinancialEntryService::new(store.clone(), registers.references());
        let dre = conta_verde_application::DreService::new(store, registers.categories.clone());
        contract::reports_the_income_statement(
            &dre,
            &entries,
            &registers.categories,
            company,
            other_company,
            &seed,
            &other_seed,
        )
        .await;
    }

    #[tokio::test]
    async fn exports_the_books_of_a_span() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let (seed, other_seed) = (
            registers.seed(company).await,
            registers.seed(other_company).await,
        );
        let store = InMemoryFinancialEntryRepository::default();
        let entries = FinancialEntryService::new(store.clone(), registers.references());
        contract::exports_the_books_of_a_span(
            &entries,
            &store,
            company,
            other_company,
            &seed,
            &other_seed,
        )
        .await;
    }

    #[tokio::test]
    async fn names_everything_for_the_export() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let (seed, other_seed) = (
            registers.seed(company).await,
            registers.seed(other_company).await,
        );
        contract::names_everything_for_the_export(
            &registers.references(),
            &registers.categories,
            company,
            other_company,
            &seed,
            &other_seed,
        )
        .await;
    }

    #[tokio::test]
    async fn an_export_past_a_limit_is_refused_and_never_cut() {
        use conta_verde_application::{
            AccountingExportService, ApplicationError, AttachmentService, ExportLimits,
        };
        use conta_verde_domain::{CalendarDate, SettlementDraft};

        use crate::{InMemoryAttachmentRepository, InMemoryBlobStore, InMemoryTransferRepository};

        let registers = Registers::new();
        let store = InMemoryFinancialEntryRepository::default();
        let entries = FinancialEntryService::new(store.clone(), registers.references());
        let attachments = InMemoryAttachmentRepository::default();
        let blobs = InMemoryBlobStore::default();
        let attach = AttachmentService::new(attachments.clone(), blobs.clone(), store.clone());
        let service = AccountingExportService::new(
            store.clone(),
            InMemoryTransferRepository::default(),
            attachments,
            blobs,
            registers.references(),
        )
        .with_limits(ExportLimits {
            rows: 3,
            attached_files: 1,
            attached_bytes: 20,
        });
        let ana = contract::actor();
        let day = |text: &str| CalendarDate::parse(text).unwrap();
        let (from, to) = (day("2026-10-01"), day("2026-10-31"));
        let too_large = |what: &'static str| ApplicationError::ExportTooLarge(what);

        // Three installments are what a file takes; a fourth is refused, not cut off.
        let company = CompanyId::generate();
        let seed = registers.seed(company).await;
        let register = |parts: &[(&str, &str)]| {
            entries.register(
                company,
                &ana,
                contract::entry_draft(&seed, "Lançamento", "2026-10-10", parts),
            )
        };
        let three = register(&[
            ("2026-10-20", "1"),
            ("2026-10-21", "1"),
            ("2026-10-22", "1"),
        ])
        .await
        .unwrap();
        assert_eq!(
            service
                .entries_table(company, from, to)
                .await
                .unwrap()
                .rows
                .len(),
            3
        );
        let one = register(&[("2026-10-23", "1")]).await.unwrap();
        for result in [
            service.entries_table(company, from, to).await.err(),
            service.package(company, from, to).await.err(),
        ] {
            assert_eq!(result, Some(too_large("more than 50,000 installments")));
        }

        // The same for settlements: three are exported, a fourth is not.
        let settle = |entry: &conta_verde_domain::FinancialEntry, installment: u16| {
            entries.settle(
                company,
                &ana,
                entry.id(),
                SettlementDraft {
                    installment,
                    settled_on: "2026-10-25".into(),
                    applied: "1".into(),
                    interest: None,
                    fine: None,
                    discount: None,
                    bank_account_id: seed.bank_account,
                    payment_method_id: None,
                },
            )
        };
        for installment in 1..=3 {
            settle(&three, installment).await.unwrap();
        }
        assert_eq!(
            service
                .settlements_table(company, from, to)
                .await
                .unwrap()
                .rows
                .len(),
            3
        );
        settle(&one, 1).await.unwrap();
        assert_eq!(
            service
                .settlements_table(company, from, to)
                .await
                .unwrap_err(),
            too_large("more than 50,000 settlements")
        );

        // Attached files: one is the limit, and 20 bytes; the records say so before any file is read.
        let attached_to = |other: CompanyId| {
            let (registers, entries, ana) = (&registers, &entries, &ana);
            async move {
                let seed = registers.seed(other).await;
                let entry = entries
                    .register(
                        other,
                        ana,
                        contract::entry_draft(
                            &seed,
                            "Com anexos",
                            "2026-10-10",
                            &[("2026-10-20", "1")],
                        ),
                    )
                    .await
                    .unwrap();
                (entry, ana.clone())
            }
        };
        let (entry, who) = attached_to(CompanyId::generate()).await;
        let two_files = entry.company_id();
        for name in ["a.pdf", "b.pdf"] {
            attach
                .attach(
                    two_files,
                    &who,
                    entry.id(),
                    name,
                    "application/pdf",
                    b"%PDF",
                )
                .await
                .unwrap();
        }
        assert_eq!(
            service.package(two_files, from, to).await.unwrap_err(),
            too_large("more than 500 attached files")
        );
        let (entry, who) = attached_to(CompanyId::generate()).await;
        let heavy = entry.company_id();
        attach
            .attach(
                heavy,
                &who,
                entry.id(),
                "c.pdf",
                "application/pdf",
                &[b'x'; 25],
            )
            .await
            .unwrap();
        assert_eq!(
            service.package(heavy, from, to).await.unwrap_err(),
            too_large("more than 200 MiB of attached files")
        );
        let (entry, who) = attached_to(CompanyId::generate()).await;
        let fits = entry.company_id();
        attach
            .attach(
                fits,
                &who,
                entry.id(),
                "d.pdf",
                "application/pdf",
                b"%PDF-1.4",
            )
            .await
            .unwrap();
        let package = service.package(fits, from, to).await.unwrap();
        assert_eq!(package.files.len(), 1);
        assert_eq!(package.files[0].bytes, b"%PDF-1.4");

        // A span of more than 366 days is a bad request, however little is in it.
        assert_eq!(
            service
                .entries_table(company, day("2026-01-01"), day("2027-01-02"))
                .await
                .unwrap_err(),
            ApplicationError::InvalidSpan("at most 366 days")
        );
    }

    #[tokio::test]
    async fn reports_the_entries() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let service = FinancialEntryService::new(
            InMemoryFinancialEntryRepository::default(),
            registers.references(),
        );
        contract::reports_the_entries(&service, company, other_company, &seed).await;
    }

    #[tokio::test]
    async fn filters_and_sorts_entries() {
        let registers = Registers::new();
        let (company, other_company) = (CompanyId::generate(), CompanyId::generate());
        let seed = registers.seed(company).await;
        let other_seed = registers.seed(other_company).await;
        let service = FinancialEntryService::new(
            InMemoryFinancialEntryRepository::default(),
            registers.references(),
        );
        contract::filters_and_sorts_entries(&service, company, other_company, &other_seed, &seed)
            .await;
    }
}
