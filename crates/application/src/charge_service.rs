use conta_verde_domain::{
    Actor, Amount, AuditAction, AuditRecord, CalendarDate, Charge, ChargeDraft, ChargeId,
    ChargeMethod, ChargeStatus, CompanyId, Decimal, DomainError, EntryId, EntryKind,
    FinancialEntry, charge_changes, charge_creation_changes, settled_of,
};

use crate::audit::audit_entry;
use crate::{
    ApplicationError, ChargeRepository, CollectionSettingsRepository, FinancialEntryRepository,
    PartyRepository, Payer, PayerAddress, PaymentProvider, ProviderChargeRequest, ProviderError,
    RepositoryError,
};

/// Charges for the installments of the receivables of a company ([ADR 0045]): the provider makes
/// them and keeps the money, this keeps what was asked and what the customer needs to pay.
pub struct ChargeService<C, E, P, V, S> {
    charges: C,
    entries: E,
    parties: P,
    provider: V,
    settings: S,
}

impl<C, E, P, V, S> ChargeService<C, E, P, V, S>
where
    C: ChargeRepository,
    E: FinancialEntryRepository,
    P: PartyRepository,
    V: PaymentProvider,
    S: CollectionSettingsRepository,
{
    pub fn new(charges: C, entries: E, parties: P, provider: V, settings: S) -> Self {
        Self {
            charges,
            entries,
            parties,
            provider,
            settings,
        }
    }

    /// The key of the provider charges are made at, or `None` when the operator configured none.
    pub fn provider_key(&self) -> Option<&str> {
        self.provider.is_enabled().then(|| self.provider.key())
    }

    /// Asks the provider to collect what is left of an installment and stores the charge, with its
    /// creation in the audit trail, in the name of `actor`. The entry must be a receivable that is not
    /// cancelled, with a customer, and the installment must have something left. The charge is for
    /// that balance and falls due on `due_on` (the installment's due date when not said).
    pub async fn issue(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        entry_id: EntryId,
        installment: u16,
        method: ChargeMethod,
        due_on: Option<CalendarDate>,
    ) -> Result<Charge, ApplicationError> {
        if !self.provider.is_enabled() {
            return Err(ProviderError::Disabled.into());
        }
        let entry = self.find_entry(company_id, entry_id).await?;
        // Without an account to land in, a payment could not be settled: nothing is asked of the provider.
        if self.settings.find(company_id).await?.is_none() {
            return Err(ApplicationError::CollectionNotConfigured);
        }
        if entry.kind() != EntryKind::Receivable {
            return Err(ApplicationError::ChargeNeedsReceivable);
        }
        if entry.is_voided() {
            return Err(ApplicationError::EntryIsVoided);
        }
        let item = entry
            .installment(installment)
            .ok_or(ApplicationError::UnusableReference("installment"))?;
        let amount = self
            .balance_of(&entry, installment, item.amount().as_decimal())
            .await?;
        let party_id = entry.party_id().ok_or(ApplicationError::ChargeNeedsParty)?;
        let party = self
            .parties
            .find(company_id, party_id)
            .await?
            .ok_or(ApplicationError::PartyNotFound)?;
        // Asked before the provider, which a double click would otherwise make a boleto for nothing; the
        // store's unique index is what holds when two requests are at it at the same time.
        if self
            .charges
            .list_by_entry(company_id, entry_id)
            .await?
            .iter()
            .any(|charge| {
                charge.installment() == installment && charge.status() == ChargeStatus::Open
            })
        {
            return Err(RepositoryError::ChargeAlreadyOpen.into());
        }
        let due_on = due_on.unwrap_or_else(|| item.due_date());
        let charge_id = ChargeId::generate();
        let made = self
            .provider
            .create_charge(&ProviderChargeRequest {
                charge_id,
                method,
                amount,
                due_on,
                payer: Payer {
                    name: party.name().to_owned(),
                    document: party
                        .document()
                        .map(|document| document.as_str().to_owned()),
                    email: party.email().map(|email| email.as_str().to_owned()),
                    address: party.address().map(|address| PayerAddress {
                        street: match address.number() {
                            Some(number) => format!("{}, {number}", address.street()),
                            None => address.street().to_owned(),
                        },
                        city: address.city().to_owned(),
                        state: address.state().to_owned(),
                        postal_code: address.postal_code().to_owned(),
                    }),
                },
                description: format!(
                    "{} ({}/{})",
                    entry.description(),
                    installment,
                    entry.installments().len()
                ),
            })
            .await?;
        let charge = match Charge::new(
            charge_id,
            company_id,
            ChargeDraft {
                entry_id,
                installment,
                method,
                amount,
                due_on,
                provider: self.provider.key().to_owned(),
                external_id: made.external_id.clone(),
                payment: made.payment,
            },
            actor.clone(),
        ) {
            Ok(charge) => charge,
            Err(error) => {
                self.take_back(&made.external_id).await;
                return Err(ProviderError::Rejected(format!(
                    "an answer that is not a charge: {error}"
                ))
                .into());
            }
        };
        let audit = audit_entry(
            &charge,
            actor,
            AuditAction::Created,
            charge_creation_changes(&charge),
        );
        if let Err(error) = self.charges.save(&charge, &audit).await {
            // Nothing of ours refers to it, so a charge left at the provider could only be paid in
            // vain: it is called off, and the error to report is the write's.
            self.take_back(charge.external_id()).await;
            return Err(error.into());
        }
        Ok(charge)
    }

    /// Calls an open charge off, at the provider first and then here. It needs the provider that made
    /// it: with another one configured, or none, the charge is left as it is.
    pub async fn cancel(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        entry_id: EntryId,
        id: ChargeId,
    ) -> Result<Charge, ApplicationError> {
        let stored = self.get(company_id, entry_id, id).await?;
        let after = stored.cancelled()?;
        if !self.provider.is_enabled() {
            return Err(ProviderError::Disabled.into());
        }
        if self.provider.key() != stored.provider() {
            return Err(ApplicationError::ChargeFromAnotherProvider);
        }
        self.provider.cancel_charge(stored.external_id()).await?;
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            charge_changes(&stored, &after),
        );
        let recorded = self
            .charges
            .cancel(&after, &audit)
            .await
            .map_err(|error| match error {
                RepositoryError::NotFound => ApplicationError::ChargeNotFound,
                other => other.into(),
            });
        match recorded {
            Ok(()) => Ok(after),
            // Another request got there first: if it called the charge off too, that is the answer.
            Err(ApplicationError::ChargeNotFound) => {
                let now = self.get(company_id, entry_id, id).await?;
                if now.status() == ChargeStatus::Cancelled {
                    Ok(now)
                } else {
                    Err(DomainError::ChargeIsNotOpen.into())
                }
            }
            Err(other) => Err(other),
        }
    }

    pub async fn get(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: ChargeId,
    ) -> Result<Charge, ApplicationError> {
        self.charges
            .find(company_id, entry_id, id)
            .await?
            .ok_or(ApplicationError::ChargeNotFound)
    }

    /// The charges of an entry of the company, in any status, in the order they were issued.
    pub async fn list(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
    ) -> Result<Vec<Charge>, ApplicationError> {
        self.find_entry(company_id, entry_id).await?;
        Ok(self.charges.list_by_entry(company_id, entry_id).await?)
    }

    /// What was recorded about the charge, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        id: ChargeId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's charge answers like an unknown one, never with its trail.
        self.get(company_id, entry_id, id).await?;
        Ok(self.charges.history(company_id, id).await?)
    }

    async fn find_entry(
        &self,
        company_id: CompanyId,
        id: EntryId,
    ) -> Result<FinancialEntry, ApplicationError> {
        self.entries
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::EntryNotFound)
    }

    /// What is left of the installment: its amount less what its settlements that are not reversed
    /// settled ([ADR 0030]). Nothing left is nothing to charge.
    async fn balance_of(
        &self,
        entry: &FinancialEntry,
        installment: u16,
        amount: Decimal,
    ) -> Result<Amount, ApplicationError> {
        let settlements = self
            .entries
            .settlements_of(entry.company_id(), &[entry.id()])
            .await?;
        let settled = settled_of(&settlements, installment);
        Amount::in_cents(&(amount - settled).to_string())
            .filter(|left| !left.as_decimal().is_zero())
            .ok_or(ApplicationError::NothingToCharge)
    }

    /// Best effort: the provider's charge is called off when ours could not be stored.
    async fn take_back(&self, external_id: &str) {
        if let Err(error) = self.provider.cancel_charge(external_id).await {
            tracing::warn!(%error, "a charge that could not be stored was left open at the provider");
        }
    }
}
