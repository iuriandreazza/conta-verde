use conta_verde_domain::{
    Charge, ChargeId, ChargeStatus, CompanyId, DomainError, EntryId, settled_of,
};

use crate::{
    ApplicationError, ChargeRepository, FinancialEntryRepository, PaymentLinkRepository,
    SecretService,
};

/// Whether the customer can still pay what the page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageState {
    /// The charge is open and its installment still owes what the charge asks.
    Payable,
    /// The charge was paid.
    Paid,
    /// Called off, expired, or the installment was settled or cancelled another way: nothing to pay.
    Unavailable,
}

/// What the customer sees at a link ([ADR 0047]): the charge, what it is for, and whether it can be paid.
#[derive(Debug, Clone)]
pub struct PaymentPage {
    pub charge: Charge,
    /// The description of the receivable, as the company typed it.
    pub description: String,
    /// How many installments the receivable has.
    pub installments: usize,
    pub state: PageState,
}

/// What the reminders need of the payment pages: a link to the open charge of an installment, if it has one.
pub trait InstallmentLinks: Send + Sync {
    /// Makes a link to the page of the open charge of this installment of the company's entry; `None` when
    /// it has no open charge. Every call makes a new link: each reminder has its own.
    fn link_for_installment(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        installment: u16,
    ) -> impl std::future::Future<Output = Result<Option<String>, ApplicationError>> + Send;
}

/// The payment page of a charge, behind a link that carries an unguessable token ([ADR 0047]).
pub struct PaymentPageService<L, C, E, S> {
    links: L,
    charges: C,
    entries: E,
    secrets: S,
    public_url: String,
}

impl<L, C, E, S> PaymentPageService<L, C, E, S>
where
    L: PaymentLinkRepository,
    C: ChargeRepository,
    E: FinancialEntryRepository,
    S: SecretService,
{
    /// `public_url` is where the web app is, so a link leads to it.
    pub fn new(links: L, charges: C, entries: E, secrets: S, public_url: &str) -> Self {
        Self {
            links,
            charges,
            entries,
            secrets,
            public_url: public_url.trim_end_matches('/').to_owned(),
        }
    }

    /// Makes a link to an open charge of the entry. The address holds the token, which is **shown once**:
    /// only its digest is kept. Another company's charge is not found, and a charge that is not open has
    /// nothing to pay.
    pub async fn new_link(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        charge_id: ChargeId,
    ) -> Result<String, ApplicationError> {
        let charge = self
            .charges
            .find(company_id, entry_id, charge_id)
            .await?
            .ok_or(ApplicationError::ChargeNotFound)?;
        if charge.status() != ChargeStatus::Open {
            return Err(DomainError::ChargeIsNotOpen.into());
        }
        let token = self.secrets.new_token();
        self.links
            .save(company_id, charge_id, &self.secrets.digest_token(&token))
            .await?;
        Ok(format!("{}/pay/{token}", self.public_url))
    }

    /// The page a link leads to; a link that was never made leads to none.
    pub async fn page(&self, token: &str) -> Result<PaymentPage, ApplicationError> {
        let target = self
            .links
            .find(&self.secrets.digest_token(token))
            .await?
            .ok_or(ApplicationError::ChargeNotFound)?;
        let charge = self
            .charges
            .find_by_id(target.company_id, target.charge_id)
            .await?
            .ok_or(ApplicationError::ChargeNotFound)?;
        let entry = self
            .entries
            .find(target.company_id, charge.entry_id())
            .await?
            .ok_or(ApplicationError::ChargeNotFound)?;
        let state = match charge.status() {
            ChargeStatus::Paid => PageState::Paid,
            ChargeStatus::Open => {
                let settlements = self
                    .entries
                    .settlements_of(target.company_id, &[entry.id()])
                    .await?;
                // The webhook settles the installment before it marks the charge paid: in between, the
                // customer who just paid is told so and not that there is nothing to pay.
                if settlements
                    .iter()
                    .any(|settlement| settlement.charge_id() == Some(charge.id()))
                {
                    PageState::Paid
                } else {
                    // Payable only while the installment still owes exactly what the charge asks: after
                    // a part was settled another way, paying the old amount would overpay.
                    let owes_the_charge =
                        entry.installment(charge.installment()).is_some_and(|item| {
                            item.amount().as_decimal()
                                - settled_of(&settlements, charge.installment())
                                == charge.amount().as_decimal()
                        });
                    if entry.is_voided() || !owes_the_charge {
                        PageState::Unavailable
                    } else {
                        PageState::Payable
                    }
                }
            }
            ChargeStatus::Cancelled | ChargeStatus::Expired => PageState::Unavailable,
        };
        Ok(PaymentPage {
            description: entry.description().to_owned(),
            installments: entry.installments().len(),
            charge,
            state,
        })
    }
}

impl<L, C, E, S> InstallmentLinks for PaymentPageService<L, C, E, S>
where
    L: PaymentLinkRepository,
    C: ChargeRepository,
    E: FinancialEntryRepository,
    S: SecretService,
{
    async fn link_for_installment(
        &self,
        company_id: CompanyId,
        entry_id: EntryId,
        installment: u16,
    ) -> Result<Option<String>, ApplicationError> {
        let open = self
            .charges
            .list_by_entry(company_id, entry_id)
            .await?
            .into_iter()
            .find(|charge| {
                charge.installment() == installment && charge.status() == ChargeStatus::Open
            });
        match open {
            Some(charge) => Ok(Some(
                self.new_link(company_id, entry_id, charge.id()).await?,
            )),
            None => Ok(None),
        }
    }
}
