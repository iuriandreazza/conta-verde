use std::collections::{HashMap, HashSet};
use std::future::Future;

use conta_verde_domain::{
    Attachment, BankAccountId, CalendarDate, CategoryId, CompanyId, CostCenterId, EntryId,
    FinancialEntry, PartyId, PaymentMethodId, Settlement,
};

use crate::{
    ApplicationError, AttachmentRepository, BlobStore, DateCursor, EntrySort, FinancialEntryQuery,
    FinancialEntryRepository, RepositoryError, TransferQuery, TransferRepository,
};

/// At most this many days from the first to the last, as the other reports of a span ([ADR 0071]).
const MAX_DAYS: i64 = 366;
/// Rows of one file; one row more is asked for, so a span that does not fit is known and never cut ([ADR 0071]).
pub const MAX_ROWS: usize = 50_000;
pub const MAX_ATTACHED_FILES: usize = 500;
pub const MAX_ATTACHED_BYTES: u64 = 200 * 1024 * 1024;
/// How many rows of a list are read at a time.
const PAGE: usize = 1_000;

/// One cell of an exported table. An amount carries the decimal text as stored: no float is ever made of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportCell {
    Text(String),
    Amount(String),
}

/// What a file of the export holds, as plain data: the layout is the one in ADR 0071 and drawn by the API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportTable {
    pub name: &'static str,
    pub header: &'static [&'static str],
    pub rows: Vec<Vec<ExportCell>>,
}

/// A file attached to an entry, with the path it has in the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// The month's package: the three tables, the list of attachments and the attached files themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportPackage {
    pub entries: ExportTable,
    pub settlements: ExportTable,
    pub transfers: ExportTable,
    pub attachments: ExportTable,
    pub files: Vec<ExportFile>,
}

/// The names the export prints for ids, **inactive records included** (an old entry may point at a category that was
/// switched off since).
#[derive(Debug, Clone, Default)]
pub struct ExportNames {
    /// `Root > Child` for a subcategory, the name for a root.
    pub categories: HashMap<CategoryId, String>,
    pub cost_centers: HashMap<CostCenterId, String>,
    pub bank_accounts: HashMap<BankAccountId, String>,
    pub payment_methods: HashMap<PaymentMethodId, String>,
    /// Name and document (without punctuation) of the parties asked for.
    pub parties: HashMap<PartyId, (String, Option<String>)>,
}

/// Where the export gets its names, asked once per request. A second narrow trait that `RegisterReferences` implements,
/// like `EntryLookups`, so one implementation serves every adapter.
pub trait ExportLookups: Send + Sync {
    fn export_names(
        &self,
        company_id: CompanyId,
        parties: &[PartyId],
    ) -> impl Future<Output = Result<ExportNames, RepositoryError>> + Send;
}

/// The books of a span as the accountant's software takes them ([ADR 0071]): entries by competence date, settlements by
/// the day they were settled, transfers by the day they occurred and the files attached to the entries. Read only.
pub struct AccountingExportService<E, T, A, B, N> {
    entries: E,
    transfers: T,
    attachments: A,
    blobs: B,
    names: N,
}

impl<E, T, A, B, N> AccountingExportService<E, T, A, B, N>
where
    E: FinancialEntryRepository,
    T: TransferRepository,
    A: AttachmentRepository,
    B: BlobStore,
    N: ExportLookups,
{
    pub fn new(entries: E, transfers: T, attachments: A, blobs: B, names: N) -> Self {
        Self {
            entries,
            transfers,
            attachments,
            blobs,
            names,
        }
    }

    pub async fn entries_table(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<ExportTable, ApplicationError> {
        check_span(from, to)?;
        let entries = self.entries_of(company_id, from, to).await?;
        let names = self.names_for(company_id, &entries, &[]).await?;
        Ok(entries_table(&entries, &names))
    }

    pub async fn settlements_table(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<ExportTable, ApplicationError> {
        check_span(from, to)?;
        self.settlements_of(company_id, from, to).await
    }

    pub async fn transfers_table(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<ExportTable, ApplicationError> {
        check_span(from, to)?;
        self.transfers_of(company_id, from, to).await
    }

    /// Everything of the span; refused, with nothing built, when a file would pass a limit.
    pub async fn package(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<ExportPackage, ApplicationError> {
        check_span(from, to)?;
        let entries = self.entries_of(company_id, from, to).await?;
        let names = self.names_for(company_id, &entries, &[]).await?;
        let entries_table = entries_table(&entries, &names);
        let settlements = self.settlements_of(company_id, from, to).await?;
        let transfers = self.transfers_of(company_id, from, to).await?;

        let ids: Vec<EntryId> = entries.iter().map(FinancialEntry::id).collect();
        let attached = self.attachments.list_by_entries(company_id, &ids).await?;
        // The limits are checked from the records, before any file is read.
        if attached.len() > MAX_ATTACHED_FILES {
            return Err(ApplicationError::ExportTooLarge(
                "more than 500 attached files",
            ));
        }
        if attached.iter().map(Attachment::size).sum::<u64>() > MAX_ATTACHED_BYTES {
            return Err(ApplicationError::ExportTooLarge(
                "more than 200 MiB of attached files",
            ));
        }
        let mut files = Vec::with_capacity(attached.len());
        let mut index = Vec::with_capacity(attached.len());
        for attachment in &attached {
            let Some(bytes) = self.blobs.get(company_id, attachment.id()).await? else {
                // The record says the file exists, so a store that lost it is a storage fault.
                return Err(
                    RepositoryError::Storage("an attached file is missing".to_owned()).into(),
                );
            };
            let path = attachment_path(attachment);
            index.push(vec![
                text(attachment.id().as_uuid()),
                text(attachment.entry_id().as_uuid()),
                text(attachment.file_name()),
                text(attachment.content_type()),
                ExportCell::Amount(attachment.size().to_string()),
                text(&path),
            ]);
            files.push(ExportFile { path, bytes });
        }
        Ok(ExportPackage {
            entries: entries_table,
            settlements,
            transfers,
            attachments: ExportTable {
                name: "attachments",
                header: &[
                    "attachment_id",
                    "entry_id",
                    "file_name",
                    "content_type",
                    "size",
                    "path",
                ],
                rows: index,
            },
            files,
        })
    }

    async fn entries_of(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<Vec<FinancialEntry>, ApplicationError> {
        let query = FinancialEntryQuery::default()
            .with_voided(false)
            .competing_between(from, to)
            .sorted(EntrySort::CompetenceAsc);
        let mut entries: Vec<FinancialEntry> = Vec::new();
        let mut installments = 0;
        let mut after = None;
        loop {
            let page = self
                .entries
                .list_by_company(company_id, &query, after.as_ref(), PAGE)
                .await?;
            installments += page
                .iter()
                .map(|entry| entry.installments().len())
                .sum::<usize>();
            if installments > MAX_ROWS {
                return Err(ApplicationError::ExportTooLarge(
                    "more than 50,000 installments",
                ));
            }
            after = page.last().map(DateCursor::from);
            let full = page.len() == PAGE;
            entries.extend(page);
            if !full {
                return Ok(entries);
            }
        }
    }

    async fn settlements_of(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<ExportTable, ApplicationError> {
        let settlements = self
            .entries
            .settlements_between(company_id, from, to, MAX_ROWS + 1)
            .await?;
        if settlements.len() > MAX_ROWS {
            return Err(ApplicationError::ExportTooLarge(
                "more than 50,000 settlements",
            ));
        }
        let ids = distinct(settlements.iter().map(Settlement::entry_id).collect());
        let entries: HashMap<EntryId, FinancialEntry> = self
            .entries
            .find_many(company_id, &ids)
            .await?
            .into_iter()
            .map(|entry| (entry.id(), entry))
            .collect();
        let parties: Vec<PartyId> = entries
            .values()
            .filter_map(FinancialEntry::party_id)
            .collect();
        let names = self
            .names
            .export_names(company_id, &distinct(parties))
            .await?;

        let mut rows = Vec::with_capacity(settlements.len());
        for settlement in &settlements {
            // The row of a settlement whose entry is gone cannot be named, and the database does not allow it.
            let entry = entries.get(&settlement.entry_id()).ok_or_else(|| {
                RepositoryError::Storage("a settlement without its entry".to_owned())
            })?;
            rows.push(vec![
                text(settlement.id().as_uuid()),
                text(entry.id().as_uuid()),
                text(entry.kind().as_str()),
                text(entry.description()),
                text(party_name(&names, entry.party_id())),
                text(category_name(&names, entry.category_id())),
                text(cost_center_name(&names, entry.cost_center_id())),
                ExportCell::Amount(settlement.installment().to_string()),
                text(settlement.settled_on()),
                text(
                    names
                        .bank_accounts
                        .get(&settlement.bank_account_id())
                        .map_or("", String::as_str),
                ),
                text(
                    settlement
                        .payment_method_id()
                        .and_then(|method| names.payment_methods.get(&method))
                        .map_or("", String::as_str),
                ),
                amount(settlement.applied()),
                amount(settlement.interest()),
                amount(settlement.fine()),
                amount(settlement.discount()),
                ExportCell::Amount(settlement.cash().normalize().to_string()),
            ]);
        }
        Ok(ExportTable {
            name: "settlements",
            header: &[
                "settlement_id",
                "entry_id",
                "kind",
                "description",
                "party",
                "category",
                "cost_center",
                "installment",
                "settled_on",
                "bank_account",
                "payment_method",
                "applied",
                "interest",
                "fine",
                "discount",
                "cash",
            ],
            rows,
        })
    }

    async fn transfers_of(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<ExportTable, ApplicationError> {
        let query = TransferQuery::default()
            .occurring_between(from, to)
            .unreversed();
        let mut transfers = Vec::new();
        let mut after = None;
        loop {
            let page = self
                .transfers
                .list_by_company(company_id, &query, after.as_ref(), PAGE)
                .await?;
            if transfers.len() + page.len() > MAX_ROWS {
                return Err(ApplicationError::ExportTooLarge(
                    "more than 50,000 transfers",
                ));
            }
            after = page.last().map(DateCursor::from);
            let full = page.len() == PAGE;
            transfers.extend(page);
            if !full {
                break;
            }
        }
        // The list is newest first; an accountant reads a month from its first day.
        transfers.sort_by_key(|transfer| (transfer.occurred_on(), transfer.id().as_uuid()));
        let names = self.names.export_names(company_id, &[]).await?;
        let account = |id| names.bank_accounts.get(&id).map_or("", String::as_str);
        let rows = transfers
            .iter()
            .map(|transfer| {
                vec![
                    text(transfer.id().as_uuid()),
                    text(transfer.occurred_on()),
                    text(account(transfer.from())),
                    text(account(transfer.to())),
                    amount(transfer.amount()),
                    text(transfer.note().unwrap_or_default()),
                ]
            })
            .collect();
        Ok(ExportTable {
            name: "transfers",
            header: &[
                "transfer_id",
                "occurred_on",
                "from_account",
                "to_account",
                "amount",
                "note",
            ],
            rows,
        })
    }

    async fn names_for(
        &self,
        company_id: CompanyId,
        entries: &[FinancialEntry],
        extra_parties: &[PartyId],
    ) -> Result<ExportNames, ApplicationError> {
        let parties: Vec<PartyId> = entries
            .iter()
            .filter_map(FinancialEntry::party_id)
            .chain(extra_parties.iter().copied())
            .collect();
        Ok(self
            .names
            .export_names(company_id, &distinct(parties))
            .await?)
    }
}

fn check_span(from: CalendarDate, to: CalendarDate) -> Result<(), ApplicationError> {
    if from > to {
        return Err(ApplicationError::InvalidSpan("from is after to"));
    }
    if to.to_unix_days() - from.to_unix_days() >= MAX_DAYS {
        return Err(ApplicationError::InvalidSpan("at most 366 days"));
    }
    Ok(())
}

/// Each id once, in the order it first appears.
fn distinct<T: Copy + Eq + std::hash::Hash>(ids: Vec<T>) -> Vec<T> {
    let mut seen = HashSet::new();
    ids.into_iter().filter(|id| seen.insert(*id)).collect()
}

fn text(value: impl ToString) -> ExportCell {
    ExportCell::Text(value.to_string())
}

fn amount(value: impl ToString) -> ExportCell {
    ExportCell::Amount(value.to_string())
}

fn party_name(names: &ExportNames, party: Option<PartyId>) -> &str {
    party
        .and_then(|id| names.parties.get(&id))
        .map_or("", |(name, _)| name.as_str())
}

fn category_name(names: &ExportNames, category: CategoryId) -> &str {
    names.categories.get(&category).map_or("", String::as_str)
}

fn cost_center_name(names: &ExportNames, cost_center: Option<CostCenterId>) -> &str {
    cost_center
        .and_then(|id| names.cost_centers.get(&id))
        .map_or("", String::as_str)
}

fn entries_table(entries: &[FinancialEntry], names: &ExportNames) -> ExportTable {
    let mut rows = Vec::new();
    for entry in entries {
        let count = entry.installments().len();
        for installment in entry.installments() {
            rows.push(vec![
                text(entry.id().as_uuid()),
                text(entry.kind().as_str()),
                text(entry.description()),
                text(party_name(names, entry.party_id())),
                text(
                    entry
                        .party_id()
                        .and_then(|id| names.parties.get(&id))
                        .and_then(|(_, document)| document.as_deref())
                        .unwrap_or_default(),
                ),
                text(category_name(names, entry.category_id())),
                text(cost_center_name(names, entry.cost_center_id())),
                text(entry.competence_date()),
                ExportCell::Amount(installment.number().to_string()),
                ExportCell::Amount(count.to_string()),
                text(installment.due_date()),
                amount(installment.amount()),
            ]);
        }
    }
    ExportTable {
        name: "entries",
        header: &[
            "entry_id",
            "kind",
            "description",
            "party",
            "party_document",
            "category",
            "cost_center",
            "competence_date",
            "installment",
            "installments",
            "due_date",
            "amount",
        ],
        rows,
    }
}

/// `attachments/{entry}/{attachment}-{name}`: the name is typed by a person, so what could leave the folder or
/// collide is turned into `_` and a leading dot is dropped (ADR 0071).
fn attachment_path(attachment: &Attachment) -> String {
    let cleaned: String = attachment
        .file_name()
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_start_matches('.');
    let name = if cleaned.is_empty() { "file" } else { cleaned };
    format!(
        "attachments/{}/{}-{name}",
        attachment.entry_id().as_uuid(),
        attachment.id().as_uuid()
    )
}
