use std::collections::{HashMap, HashSet};
use std::future::Future;

use conta_verde_domain::{
    Attachment, BankAccountId, CalendarDate, CategoryId, CompanyId, CostCenterId, EntryId,
    FinancialEntry, PartyId, PaymentMethodId, Settlement, Transfer,
};

use crate::{
    ApplicationError, AttachmentRepository, BlobStore, DateCursor, EntrySort, FinancialEntryQuery,
    FinancialEntryRepository, RepositoryError, TransferQuery, TransferRepository,
};

/// At most this many days from the first to the last, as the other reports of a span ([ADR 0071]).
const MAX_DAYS: i64 = 366;
/// How many rows of a list are read at a time.
const PAGE: usize = 1_000;

/// What one export takes before it answers `422` instead of cutting a file ([ADR 0071]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportLimits {
    /// Rows of one file: one row more is asked for, so a span that does not fit is known and never cut.
    pub rows: usize,
    pub attached_files: usize,
    pub attached_bytes: u64,
}

impl Default for ExportLimits {
    fn default() -> Self {
        Self {
            rows: 50_000,
            attached_files: 500,
            attached_bytes: 200 * 1024 * 1024,
        }
    }
}

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
    /// The whole path of a subcategory, `Root > Child > Grandchild`; the name for a root.
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

/// What the export reads before it names anything.
struct Books {
    entries: Vec<FinancialEntry>,
    settlements: Vec<Settlement>,
    /// The entries the settlements pay, wherever their competence date is.
    settled: HashMap<EntryId, FinancialEntry>,
    transfers: Vec<Transfer>,
}

/// The books of a span as the accountant's software takes them ([ADR 0071]): entries by competence date, settlements by
/// the day they were settled, transfers by the day they occurred and the files attached to the entries. Read only.
pub struct AccountingExportService<E, T, A, B, N> {
    entries: E,
    transfers: T,
    attachments: A,
    blobs: B,
    names: N,
    limits: ExportLimits,
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
            limits: ExportLimits::default(),
        }
    }

    /// Other limits than the ones of the ADR: for a test that cannot make fifty thousand rows.
    pub fn with_limits(mut self, limits: ExportLimits) -> Self {
        self.limits = limits;
        self
    }

    pub async fn entries_table(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<ExportTable, ApplicationError> {
        check_span(from, to)?;
        let entries = self.gather_entries(company_id, from, to).await?;
        let names = self.names_of(company_id, &entries, &[]).await?;
        Ok(entries_table(&entries, &names))
    }

    pub async fn settlements_table(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<ExportTable, ApplicationError> {
        check_span(from, to)?;
        let (settlements, settled) = self.gather_settlements(company_id, from, to).await?;
        let names = self.names_of(company_id, &[], &[&settled]).await?;
        Ok(settlements_table(&settlements, &settled, &names)?)
    }

    pub async fn transfers_table(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<ExportTable, ApplicationError> {
        check_span(from, to)?;
        let transfers = self.gather_transfers(company_id, from, to).await?;
        let names = self.names_of(company_id, &[], &[]).await?;
        Ok(transfers_table(&transfers, &names))
    }

    /// Everything of the span; refused, with nothing built, when a file would pass a limit.
    pub async fn package(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<ExportPackage, ApplicationError> {
        check_span(from, to)?;
        let entries = self.gather_entries(company_id, from, to).await?;
        let (settlements, settled) = self.gather_settlements(company_id, from, to).await?;
        let books = Books {
            entries,
            settlements,
            settled,
            transfers: self.gather_transfers(company_id, from, to).await?,
        };
        // The registers are read once for the whole package.
        let names = self
            .names_of(company_id, &books.entries, &[&books.settled])
            .await?;

        let (attachments, files) = self.gather_attachments(company_id, &books.entries).await?;
        Ok(ExportPackage {
            entries: entries_table(&books.entries, &names),
            settlements: settlements_table(&books.settlements, &books.settled, &names)?,
            transfers: transfers_table(&books.transfers, &names),
            attachments,
            files,
        })
    }

    async fn gather_entries(
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
            if installments > self.limits.rows {
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

    async fn gather_settlements(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<(Vec<Settlement>, HashMap<EntryId, FinancialEntry>), ApplicationError> {
        let settlements = self
            .entries
            .settlements_between(company_id, from, to, self.limits.rows + 1)
            .await?;
        if settlements.len() > self.limits.rows {
            return Err(ApplicationError::ExportTooLarge(
                "more than 50,000 settlements",
            ));
        }
        let ids = distinct(settlements.iter().map(Settlement::entry_id).collect());
        let settled = self
            .entries
            .find_many(company_id, &ids)
            .await?
            .into_iter()
            .map(|entry| (entry.id(), entry))
            .collect();
        Ok((settlements, settled))
    }

    async fn gather_transfers(
        &self,
        company_id: CompanyId,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<Vec<Transfer>, ApplicationError> {
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
            if transfers.len() + page.len() > self.limits.rows {
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
        Ok(transfers)
    }

    /// The list of the attached files of these entries and the files, in the order of the entries (and, within an entry,
    /// the order they were attached). Refused before any file is read when the records already pass a limit.
    async fn gather_attachments(
        &self,
        company_id: CompanyId,
        entries: &[FinancialEntry],
    ) -> Result<(ExportTable, Vec<ExportFile>), ApplicationError> {
        let ids: Vec<EntryId> = entries.iter().map(FinancialEntry::id).collect();
        let mut attached = self.attachments.list_by_entries(company_id, &ids).await?;
        if attached.len() > self.limits.attached_files {
            return Err(ApplicationError::ExportTooLarge(
                "more than 500 attached files",
            ));
        }
        if attached.iter().map(Attachment::size).sum::<u64>() > self.limits.attached_bytes {
            return Err(ApplicationError::ExportTooLarge(
                "more than 200 MiB of attached files",
            ));
        }
        let position: HashMap<EntryId, usize> = ids
            .iter()
            .enumerate()
            .map(|(place, id)| (*id, place))
            .collect();
        // A stable sort: the adapters answer by entry id, the package by the order of the entries file.
        attached.sort_by_key(|attachment| {
            position
                .get(&attachment.entry_id())
                .copied()
                .unwrap_or(usize::MAX)
        });

        let mut files = Vec::with_capacity(attached.len());
        let mut index = Vec::with_capacity(attached.len());
        for attachment in &attached {
            let Some(bytes) = self.blobs.get(company_id, attachment.id().into()).await? else {
                // The record says the file exists, so a store that lost it is a storage fault: logged with which.
                return Err(RepositoryError::Storage(format!(
                    "the file of attachment {} is missing",
                    attachment.id().as_uuid()
                ))
                .into());
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
        let table = ExportTable {
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
        };
        Ok((table, files))
    }

    /// The registers' names, with the parties of these entries and of the entries paid by the settlements.
    async fn names_of(
        &self,
        company_id: CompanyId,
        entries: &[FinancialEntry],
        paid: &[&HashMap<EntryId, FinancialEntry>],
    ) -> Result<ExportNames, ApplicationError> {
        let parties: Vec<PartyId> = entries
            .iter()
            .chain(paid.iter().flat_map(|settled| settled.values()))
            .filter_map(FinancialEntry::party_id)
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

fn settlements_table(
    settlements: &[Settlement],
    settled: &HashMap<EntryId, FinancialEntry>,
    names: &ExportNames,
) -> Result<ExportTable, RepositoryError> {
    let mut rows = Vec::with_capacity(settlements.len());
    for settlement in settlements {
        // The row of a settlement whose entry is gone cannot be named, and the database does not allow it.
        let entry = settled
            .get(&settlement.entry_id())
            .ok_or_else(|| RepositoryError::Storage("a settlement without its entry".to_owned()))?;
        rows.push(vec![
            text(settlement.id().as_uuid()),
            text(entry.id().as_uuid()),
            text(entry.kind().as_str()),
            text(entry.description()),
            text(party_name(names, entry.party_id())),
            text(category_name(names, entry.category_id())),
            text(cost_center_name(names, entry.cost_center_id())),
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

fn transfers_table(transfers: &[Transfer], names: &ExportNames) -> ExportTable {
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
    ExportTable {
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
    }
}

/// `attachments/{entry}/{attachment}-{name}`: the name is typed by a person, so what could leave the folder, collide or
/// not be unpacked on Windows (where the accountant most likely is) is turned into `_`, and a leading dot or a trailing
/// dot or space is dropped ([ADR 0071]).
fn attachment_path(attachment: &Attachment) -> String {
    let cleaned: String = attachment
        .file_name()
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_start_matches('.').trim_end_matches(['.', ' ']);
    let name = if cleaned.is_empty() { "file" } else { cleaned };
    format!(
        "attachments/{}/{}-{name}",
        attachment.entry_id().as_uuid(),
        attachment.id().as_uuid()
    )
}

#[cfg(test)]
mod tests {
    use conta_verde_domain::{AttachmentDraft, AttachmentId};

    use super::*;

    fn path_of(name: &str) -> String {
        let (entry, id) = (EntryId::generate(), AttachmentId::generate());
        let attachment = Attachment::new(
            id,
            CompanyId::generate(),
            entry,
            AttachmentDraft {
                file_name: name.to_owned(),
                content_type: "application/pdf".to_owned(),
                size: 1,
            },
        )
        .unwrap();
        let prefix = format!("attachments/{}/{}-", entry.as_uuid(), id.as_uuid());
        attachment_path(&attachment)
            .strip_prefix(&prefix)
            .expect("the path is the entry's folder and the attachment's id")
            .to_owned()
    }

    // `/`, `\\` and control characters never get here: the domain refuses them in a name (ADR 0036). They stay in the
    // replacement as a second wall.
    #[test]
    fn a_name_that_windows_cannot_unpack_is_made_one_that_it_can() {
        assert_eq!(path_of("Nota ç.pdf"), "Nota ç.pdf");
        assert_eq!(
            path_of("Nota fiscal: outubro.pdf"),
            "Nota fiscal_ outubro.pdf"
        );
        assert_eq!(path_of("Boleto?*\"<>|.pdf"), "Boleto______.pdf");
    }

    #[test]
    fn a_leading_dot_and_a_trailing_dot_or_space_are_dropped_and_nothing_is_left_nameless() {
        assert_eq!(path_of(".hidden.pdf"), "hidden.pdf");
        assert_eq!(path_of("nota.pdf. ."), "nota.pdf");
        assert_eq!(path_of("..."), "file");
    }

    #[test]
    fn a_span_is_at_most_366_days_both_ends_in() {
        let day = |text: &str| CalendarDate::parse(text).unwrap();
        assert!(check_span(day("2026-01-01"), day("2026-01-01")).is_ok());
        assert!(check_span(day("2026-01-01"), day("2027-01-01")).is_ok());
        assert_eq!(
            check_span(day("2026-01-01"), day("2027-01-02")),
            Err(ApplicationError::InvalidSpan("at most 366 days"))
        );
        assert_eq!(
            check_span(day("2026-01-02"), day("2026-01-01")),
            Err(ApplicationError::InvalidSpan("from is after to"))
        );
    }
}
