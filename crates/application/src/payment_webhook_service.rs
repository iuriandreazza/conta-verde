use conta_verde_domain::{
    Amount, AuditAction, CalendarDate, Charge, ChargeReceipt, ChargeStatus, CollectionSettings,
    CompanyId, Decimal, EntryInstallmentDraft, EntryKind, EntryOrigin, FinancialEntry,
    FinancialEntryDraft, PaymentEvent, PaymentEventId, PaymentEventKind, PaymentEventOutcome,
    SettlementDraft, charge_changes, event_note, settled_of,
};

use crate::audit::audit_entry;
use crate::{
    ApplicationError, ChargeRepository, CollectionSettingsRepository, EntryReferences,
    FinancialEntryRepository, FinancialEntryService, PaymentEventRepository, PaymentProvider,
    ProviderEventKind, Recorded, RepositoryError, WebhookHeaders,
};

/// How much of an entry's description the fee's own description carries.
const FEE_DESCRIPTION_CHARS: usize = 120;

/// What a notice came to: whether it was applied, and why not when it was not.
type Applied = (PaymentEventOutcome, Option<String>);

/// Applies what a payment provider reports about the charges of a company ([ADR 0046]): a payment settles
/// the installment through the one settlement path of ADR 0030 and enters the provider's fee, and every
/// step is idempotent, so a notice that arrives twice, or half-processed after a crash, is finished and
/// never repeated.
pub struct PaymentWebhookService<V, C, Ev, S, E, F> {
    provider: V,
    charges: C,
    events: Ev,
    settings: S,
    entries: FinancialEntryService<E, F>,
}

impl<V, C, Ev, S, E, F> PaymentWebhookService<V, C, Ev, S, E, F>
where
    V: PaymentProvider,
    C: ChargeRepository,
    Ev: PaymentEventRepository,
    S: CollectionSettingsRepository,
    E: FinancialEntryRepository,
    F: EntryReferences,
{
    pub fn new(
        provider: V,
        charges: C,
        events: Ev,
        settings: S,
        entries: FinancialEntryService<E, F>,
    ) -> Self {
        Self {
            provider,
            charges,
            events,
            settings,
            entries,
        }
    }

    /// Takes a webhook the provider called the company's address with. The signature is checked over the
    /// body before anything else (`Webhook`); a provider other than the configured one, or a charge that is
    /// not the company's, is not found, and **nothing of it is stored**. A storage failure is returned so
    /// the provider sends the notice again: it stays pending until it has been applied. Answers how many
    /// notices the request held.
    pub async fn receive(
        &self,
        company_id: CompanyId,
        provider_key: &str,
        headers: &WebhookHeaders,
        body: &[u8],
    ) -> Result<usize, ApplicationError> {
        if !self.provider.is_enabled() || self.provider.key() != provider_key {
            return Err(ApplicationError::ChargeNotFound);
        }
        let notices = self.provider.verify_webhook(headers, body)?;
        let mut taken = 0;
        for notice in &notices {
            // A notice of a charge that is not the company's is left out; if none is, the request is `404`.
            let Some(charge) = self
                .charges
                .find_by_external(company_id, provider_key, &notice.external_id)
                .await?
            else {
                continue;
            };
            taken += 1;
            let event = PaymentEvent::new(
                PaymentEventId::generate(),
                company_id,
                charge.id(),
                provider_key,
                &notice.event_id,
                match notice.kind {
                    ProviderEventKind::Paid { .. } => PaymentEventKind::Paid,
                    ProviderEventKind::Cancelled => PaymentEventKind::Cancelled,
                    ProviderEventKind::Expired => PaymentEventKind::Expired,
                },
                body.to_vec(),
            )?;
            let (event, retry) = match self.events.record(&event).await? {
                Recorded::New(event) => (event, false),
                Recorded::Existing(old) if old.outcome() != PaymentEventOutcome::Pending => {
                    continue;
                }
                // Stored, and interrupted before it was finished: do it again, step by step.
                Recorded::Existing(old) => (old, true),
            };
            let (outcome, note) = self.apply(&charge, &notice.kind, retry).await?;
            self.events
                .finish(company_id, event.id(), outcome, note.as_deref())
                .await?;
        }
        // A notice the provider sends that changes nothing here is acknowledged; one that is all of other
        // companies' charges is not.
        if taken == 0 && !notices.is_empty() {
            return Err(ApplicationError::ChargeNotFound);
        }
        Ok(taken)
    }

    async fn apply(
        &self,
        charge: &Charge,
        kind: &ProviderEventKind,
        retry: bool,
    ) -> Result<Applied, ApplicationError> {
        match kind {
            ProviderEventKind::Cancelled => {
                if charge.status() == ChargeStatus::Open {
                    let after = charge.cancelled()?;
                    let audit = self.audit_move(charge, &after);
                    // Called off by someone else meanwhile is the same result.
                    ignore_missing(self.charges.cancel(&after, &audit).await)?;
                }
                Ok((PaymentEventOutcome::Applied, None))
            }
            ProviderEventKind::Expired => {
                if charge.status() == ChargeStatus::Open {
                    let after = charge.expired()?;
                    let audit = self.audit_move(charge, &after);
                    ignore_missing(self.charges.expire(&after, &audit).await)?;
                }
                Ok((PaymentEventOutcome::Applied, None))
            }
            ProviderEventKind::Paid {
                paid_amount,
                paid_on,
                fee,
            } => {
                // A second payment of a charge that was paid is kept and flagged, never settled again:
                // the money is the provider's to give back. The same notice interrupted is not that.
                // A notice of a payment of nothing is no payment: it is kept and flagged.
                if paid_amount.as_decimal().is_zero() {
                    return Ok((
                        PaymentEventOutcome::NotApplied,
                        Some("the notice says nothing was paid".to_owned()),
                    ));
                }
                if charge.status() == ChargeStatus::Paid && !retry {
                    return Ok((
                        PaymentEventOutcome::NotApplied,
                        Some("the charge was already paid".to_owned()),
                    ));
                }
                self.apply_payment(charge, *paid_amount, *paid_on, *fee)
                    .await
            }
        }
    }

    async fn apply_payment(
        &self,
        charge: &Charge,
        paid_amount: Amount,
        paid_on: CalendarDate,
        fee: Amount,
    ) -> Result<Applied, ApplicationError> {
        let company_id = charge.company_id();
        let mut notes: Vec<String> = Vec::new();
        match self.settings.find(company_id).await? {
            None => notes.push("the account charges are paid into is not set".to_owned()),
            Some(settings) => {
                let entry = self.entries.get(company_id, charge.entry_id()).await?;
                notes.extend(
                    self.settle(&entry, charge, &settings, paid_amount, paid_on)
                        .await?,
                );
                notes.extend(
                    self.enter_fee(&entry, charge, &settings, fee, paid_on)
                        .await?,
                );
            }
        }
        // Cut to what a note holds, so a long reason cannot keep the charge from being marked paid.
        let note = event_note((!notes.is_empty()).then(|| notes.join("; ")).as_deref());
        if charge.status() != ChargeStatus::Paid {
            let paid = charge.paid(ChargeReceipt {
                paid_on,
                paid_amount,
                fee,
                note: note.clone(),
            })?;
            let audit = self.audit_move(charge, &paid);
            ignore_missing(self.charges.mark_paid(&paid, &audit).await)?;
        }
        let outcome = if note.is_none() {
            PaymentEventOutcome::Applied
        } else {
            PaymentEventOutcome::NotApplied
        };
        Ok((outcome, note))
    }

    /// Settles what is left of the installment, up to the charge, with what the customer paid over or under
    /// it as interest or discount, so the cash of the settlement is exactly what was paid. `None` when it
    /// did (or already had); otherwise why it could not.
    async fn settle(
        &self,
        entry: &FinancialEntry,
        charge: &Charge,
        settings: &CollectionSettings,
        paid_amount: Amount,
        paid_on: CalendarDate,
    ) -> Result<Option<String>, ApplicationError> {
        let company_id = charge.company_id();
        if entry.is_voided() {
            return Ok(Some("the entry was cancelled".to_owned()));
        }
        let Some(item) = entry.installment(charge.installment()) else {
            return Ok(Some("the installment no longer exists".to_owned()));
        };
        let settlements = self.entries.settlements(company_id, entry.id()).await?;
        // This charge settled before (the notice was interrupted after that, or it was reversed since):
        // a charge settles once.
        if settlements
            .iter()
            .any(|settlement| settlement.charge_id() == Some(charge.id()))
        {
            return Ok(None);
        }
        let left = item.amount().as_decimal() - settled_of(&settlements, charge.installment());
        let applied = charge.amount().as_decimal().min(left);
        if applied <= Decimal::ZERO {
            return Ok(Some("the installment was already settled".to_owned()));
        }
        let paid = paid_amount.as_decimal();
        let extra = |amount: Decimal| (!amount.is_zero()).then(|| amount.normalize().to_string());
        let draft = SettlementDraft {
            installment: charge.installment(),
            settled_on: paid_on.to_string(),
            applied: applied.normalize().to_string(),
            interest: extra((paid - applied).max(Decimal::ZERO)),
            fine: None,
            discount: extra((applied - paid).max(Decimal::ZERO)),
            bank_account_id: settings.bank_account_id(),
            payment_method_id: None,
        };
        settled_or_why(
            self.entries
                .settle_charge(
                    company_id,
                    charge.issued_by(),
                    entry.id(),
                    draft,
                    charge.id(),
                )
                .await,
        )
    }

    /// Enters the fee the provider kept as a payable settled the same day from the same account, so the
    /// account moves by what the provider deposited. Without a fee category it is only on the charge.
    async fn enter_fee(
        &self,
        entry: &FinancialEntry,
        charge: &Charge,
        settings: &CollectionSettings,
        fee: Amount,
        paid_on: CalendarDate,
    ) -> Result<Option<String>, ApplicationError> {
        let company_id = charge.company_id();
        let Some(category) = settings.fee_category_id() else {
            return Ok(None);
        };
        if fee.as_decimal().is_zero() {
            return Ok(None);
        }
        let origin = EntryOrigin::ChargeFee(charge.id());
        let day = paid_on.to_string();
        let draft = FinancialEntryDraft {
            kind: EntryKind::Payable,
            description: format!(
                "Taxa de cobrança: {} ({}/{})",
                entry
                    .description()
                    .chars()
                    .take(FEE_DESCRIPTION_CHARS)
                    .collect::<String>(),
                charge.installment(),
                entry.installments().len()
            ),
            party_id: None,
            category_id: category,
            cost_center_id: None,
            competence_date: day.clone(),
            installments: vec![EntryInstallmentDraft {
                due_date: day.clone(),
                amount: fee.to_string(),
            }],
        };
        let fee_entry = match self
            .entries
            .register_with_origin(company_id, charge.issued_by(), draft, origin)
            .await
        {
            Ok(made) => made,
            Err(ApplicationError::Repository(RepositoryError::DuplicateOrigin)) => self
                .entries
                .made_by(company_id, origin)
                .await?
                .ok_or(ApplicationError::EntryNotFound)?,
            Err(ApplicationError::UnusableReference(_)) => {
                return Ok(Some("the fee category can no longer be used".to_owned()));
            }
            Err(other) => return Err(other),
        };
        if !self
            .entries
            .settlements(company_id, fee_entry.id())
            .await?
            .is_empty()
        {
            return Ok(None);
        }
        let draft = SettlementDraft {
            installment: 1,
            settled_on: day,
            applied: fee.to_string(),
            interest: None,
            fine: None,
            discount: None,
            bank_account_id: settings.bank_account_id(),
            payment_method_id: None,
        };
        match self
            .entries
            .settle(company_id, charge.issued_by(), fee_entry.id(), draft)
            .await
        {
            // Another request settled the fee between the check and the write: it is settled.
            Err(ApplicationError::SettlementExceedsBalance) => Ok(None),
            other => settled_or_why(other.map(|_| ())),
        }
    }

    fn audit_move(&self, before: &Charge, after: &Charge) -> conta_verde_domain::AuditEntry {
        audit_entry(
            after,
            before.issued_by(),
            AuditAction::Updated,
            charge_changes(before, after),
        )
    }
}

/// A charge moved by someone else meanwhile is already where this wanted it.
fn ignore_missing(result: Result<(), RepositoryError>) -> Result<(), ApplicationError> {
    match result {
        Ok(()) | Err(RepositoryError::NotFound) => Ok(()),
        Err(other) => Err(other.into()),
    }
}

/// What a refused settlement means for the notice: the refusals that say the money cannot be put where
/// it was meant to go are kept as the reason, anything else (storage) is for the provider to retry.
fn settled_or_why<T>(
    result: Result<T, ApplicationError>,
) -> Result<Option<String>, ApplicationError> {
    match result {
        Ok(_) | Err(ApplicationError::Repository(RepositoryError::DuplicateOrigin)) => Ok(None),
        Err(ApplicationError::SettlementExceedsBalance) => {
            Ok(Some("the installment was settled meanwhile".to_owned()))
        }
        Err(ApplicationError::EntryIsVoided) => Ok(Some("the entry was cancelled".to_owned())),
        Err(ApplicationError::UnusableReference(what)) => {
            Ok(Some(format!("the {what} can no longer be used")))
        }
        Err(ApplicationError::Domain(error)) => Ok(Some(error.to_string())),
        Err(other) => Err(other),
    }
}
