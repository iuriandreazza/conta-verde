use std::collections::HashMap;
use std::future::Future;

use conta_verde_domain::{
    Actor, Amount, BankAccountId, CategoryId, CompanyId, CostCenterId, DomainError, DreGroup,
    EntryId, EntryInstallmentDraft, EntryKind, EntryOrigin, FinancialEntry, FinancialEntryDraft,
    ImportId, PartyId, Settlement, SettlementDraft, SettlementId,
};

use crate::{
    ApplicationError, EntryReferences, FinancialEntryRepository, FinancialEntryService,
    ImportReport, RepositoryError, RowNote,
};

/// The most rows one request takes: each is written in turn, so a longer file is sent in pieces.
pub const MAX_ENTRY_IMPORT_ROWS: usize = 2000;

/// How a row of the file says it was paid: one installment settled on a day, in one bank account.
#[derive(Debug, Clone)]
pub struct SettlementImport {
    pub settled_on: String,
    /// What it settles of the installment; the whole installment when the file says nothing.
    pub applied: Option<String>,
    pub interest: Option<String>,
    pub fine: Option<String>,
    pub discount: Option<String>,
    pub bank_account: String,
}

/// A row of an entries file ([ADR 0064]) as the text it holds: one entry with one installment,
/// paid or open. Everything it points at is named, and the use case finds it.
#[derive(Debug, Clone)]
pub struct EntryImportDraft {
    pub reference: String,
    pub kind: String,
    pub description: String,
    pub party_document: Option<String>,
    pub party_name: Option<String>,
    /// The full path, `Root > Child`; a root is just its name, since names repeat across trees.
    pub category: String,
    pub cost_center: Option<String>,
    pub competence_date: String,
    pub due_date: String,
    pub amount: String,
    pub settlement: Option<SettlementImport>,
}

/// A party the file may mean, with the roles that tell a customer's receivable from a supplier's
/// payable when two parties share a name.
#[derive(Debug, Clone, Copy)]
struct PartyChoice {
    id: PartyId,
    is_customer: bool,
    is_supplier: bool,
}

/// What a file's names stand for in one company: the active registers an entry may point at.
#[derive(Debug, Clone, Default)]
pub struct ImportNames {
    categories: HashMap<String, CategoryId>,
    cost_centers: HashMap<String, CostCenterId>,
    bank_accounts: HashMap<String, BankAccountId>,
    parties_by_document: HashMap<String, PartyId>,
    parties_by_name: HashMap<String, Vec<PartyChoice>>,
}

fn key(text: &str) -> String {
    text.trim().to_lowercase()
}

impl ImportNames {
    /// An active category under the path `Root > Child`, never the one for transfers.
    pub fn add_category(&mut self, path: &str, id: CategoryId, group: DreGroup) {
        if group != DreGroup::Transfers {
            self.categories.insert(key(path), id);
        }
    }

    pub fn add_cost_center(&mut self, name: &str, id: CostCenterId) {
        self.cost_centers.insert(key(name), id);
    }

    pub fn add_bank_account(&mut self, name: &str, id: BankAccountId) {
        self.bank_accounts.insert(key(name), id);
    }

    pub fn add_party(
        &mut self,
        name: &str,
        document: Option<&str>,
        id: PartyId,
        is_customer: bool,
        is_supplier: bool,
    ) {
        if let Some(document) = document {
            self.parties_by_document.insert(document.to_owned(), id);
        }
        self.parties_by_name
            .entry(key(name))
            .or_default()
            .push(PartyChoice {
                id,
                is_customer,
                is_supplier,
            });
    }

    fn party(
        &self,
        document: Option<&str>,
        name: Option<&str>,
        kind: EntryKind,
    ) -> Result<Option<PartyId>, String> {
        let normalized: Option<String> = document.map(|text| {
            text.chars()
                .filter(char::is_ascii_alphanumeric)
                .collect::<String>()
                .to_uppercase()
        });
        if let Some(id) = normalized
            .as_deref()
            .filter(|text| !text.is_empty())
            .and_then(|text| self.parties_by_document.get(text))
        {
            return Ok(Some(*id));
        }
        let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) else {
            return match normalized.filter(|text| !text.is_empty()) {
                Some(_) => Err("no party has that document".to_owned()),
                None => Ok(None),
            };
        };
        let candidates = self
            .parties_by_name
            .get(&key(name))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let plays_role = |choice: &&PartyChoice| match kind {
            EntryKind::Receivable => choice.is_customer,
            EntryKind::Payable => choice.is_supplier,
        };
        let by_role: Vec<&PartyChoice> = candidates.iter().filter(plays_role).collect();
        // Several parties may share a name; the one that plays the role of this entry's kind is meant.
        match (by_role.as_slice(), candidates) {
            ([one], _) => Ok(Some(one.id)),
            ([], [one]) => Ok(Some(one.id)),
            ([], []) => Err(format!("no party is named \"{name}\"")),
            _ => Err(format!("more than one party is named \"{name}\"")),
        }
    }
}

/// The registers a file's names are looked up in, answered by asking the registers themselves, like
/// [`EntryReferences`].
pub trait EntryLookups: Send + Sync {
    /// The active categories (by path), cost centers and bank accounts of the company, and its parties.
    fn import_names(
        &self,
        company_id: CompanyId,
    ) -> impl Future<Output = Result<ImportNames, RepositoryError>> + Send;
}

/// One row, checked and resolved to what is written.
struct Prepared {
    row: usize,
    import_id: ImportId,
    draft: FinancialEntryDraft,
    settlement: Option<SettlementDraft>,
}

fn prepared(
    company_id: CompanyId,
    names: &ImportNames,
    row: usize,
    draft: EntryImportDraft,
) -> Result<Prepared, String> {
    let import_id = ImportId::new(company_id, &draft.reference).map_err(|e| e.to_string())?;
    let kind = EntryKind::parse(draft.kind.trim())
        .ok_or_else(|| "kind must be receivable or payable".to_owned())?;
    let category_id = *names
        .categories
        .get(&key(&draft.category))
        .ok_or_else(|| format!("no active category \"{}\"", draft.category.trim()))?;
    let cost_center_id = draft
        .cost_center
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| {
            names
                .cost_centers
                .get(&key(name))
                .copied()
                .ok_or_else(|| format!("no active cost center \"{name}\""))
        })
        .transpose()?;
    let party_id = names.party(
        draft.party_document.as_deref(),
        draft.party_name.as_deref(),
        kind,
    )?;
    let entry = FinancialEntryDraft {
        kind,
        description: draft.description,
        party_id,
        category_id,
        cost_center_id,
        competence_date: draft.competence_date,
        installments: vec![EntryInstallmentDraft {
            due_date: draft.due_date,
            amount: draft.amount.clone(),
        }],
    };
    FinancialEntry::new(EntryId::generate(), company_id, entry.clone())
        .map_err(|error| error.to_string())?;
    let settlement = draft
        .settlement
        .map(|settled| settlement_of(company_id, names, &draft.amount, settled))
        .transpose()?;
    Ok(Prepared {
        row,
        import_id,
        draft: entry,
        settlement,
    })
}

fn settlement_of(
    company_id: CompanyId,
    names: &ImportNames,
    installment_amount: &str,
    settled: SettlementImport,
) -> Result<SettlementDraft, String> {
    let bank_account_id = names
        .bank_accounts
        .get(&key(&settled.bank_account))
        .copied()
        .ok_or_else(|| format!("no active bank account \"{}\"", settled.bank_account.trim()))?;
    let applied = settled
        .applied
        .unwrap_or_else(|| installment_amount.to_owned());
    let draft = SettlementDraft {
        installment: 1,
        settled_on: settled.settled_on,
        applied,
        interest: settled.interest,
        fine: settled.fine,
        discount: settled.discount,
        bank_account_id,
        payment_method_id: None,
    };
    Settlement::new(
        SettlementId::generate(),
        company_id,
        EntryId::generate(),
        draft.clone(),
    )
    .map_err(|error| error.to_string())?;
    // The write would be refused for this, and by then the rows before it would be stored.
    let fits = Amount::parse(&draft.applied)
        .zip(Amount::parse(installment_amount))
        .is_some_and(|(applied, amount)| applied.as_decimal() <= amount.as_decimal());
    if !fits {
        return Err(DomainError::InvalidSettlementAmount("applied").to_string()
            + ", or more than the installment");
    }
    Ok(draft)
}

impl<R: FinancialEntryRepository, F: EntryReferences + EntryLookups> FinancialEntryService<R, F> {
    /// Registers the rows of an entries file ([ADR 0064]), or with `dry_run` only says what would happen.
    ///
    /// Every row is checked before any is written: if one is refused nothing is. A row whose reference
    /// was imported before is skipped, and if that entry never got its settlement (a failure between the
    /// two writes) the settlement is made now, so sending the file again finishes what a failure left.
    pub async fn import_entries(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        rows: Vec<(usize, EntryImportDraft)>,
        dry_run: bool,
    ) -> Result<ImportReport, ApplicationError> {
        let names = self.references().import_names(company_id).await?;
        let mut report = ImportReport::default();
        let mut ready = Vec::with_capacity(rows.len());
        let mut first_row_of: HashMap<ImportId, usize> = HashMap::new();
        for (row, draft) in rows {
            let prepared = match prepared(company_id, &names, row, draft) {
                Ok(prepared) => prepared,
                Err(message) => {
                    report.refused.push(RowNote { row, message });
                    continue;
                }
            };
            if let Some(first) = first_row_of.insert(prepared.import_id, row) {
                report.refused.push(RowNote {
                    row,
                    message: format!("the same reference as row {first}"),
                });
                continue;
            }
            ready.push(prepared);
        }
        report.valid = ready.len();
        if !report.refused.is_empty() {
            return Ok(report);
        }

        for item in ready {
            let origin = EntryOrigin::Import(item.import_id);
            let existing = self.made_by(company_id, origin).await?;
            if existing.is_some() && dry_run {
                report.skipped.push(already_imported(item.row));
                continue;
            }
            if dry_run {
                continue;
            }
            let (entry, created) = match existing {
                Some(entry) => (entry, false),
                None => match self
                    .register_with_origin(company_id, actor, item.draft, origin)
                    .await
                {
                    Ok(entry) => (entry, true),
                    Err(ApplicationError::Repository(RepositoryError::DuplicateOrigin)) => {
                        let entry = self
                            .made_by(company_id, origin)
                            .await?
                            .ok_or(ApplicationError::EntryNotFound)?;
                        (entry, false)
                    }
                    Err(error) => return Err(error),
                },
            };
            if let Some(settlement) = item.settlement
                && self.settlements(company_id, entry.id()).await?.is_empty()
            {
                self.settle(company_id, actor, entry.id(), settlement)
                    .await?;
            }
            if created {
                report.created += 1;
            } else {
                report.skipped.push(already_imported(item.row));
            }
        }
        Ok(report)
    }
}

fn already_imported(row: usize) -> RowNote {
    RowNote {
        row,
        message: RepositoryError::DuplicateOrigin.to_string(),
    }
}
