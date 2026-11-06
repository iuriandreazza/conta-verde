use conta_verde_domain::{
    Actor, AuditAction, AuditRecord, CalendarDate, CompanyId, Email, FieldChange, Quote,
    QuoteDraft, QuoteId, quote_changes, quote_creation_changes,
};

use crate::audit::audit_entry;
use crate::list_query::{clamp_limit, split_page};
use crate::{
    ApplicationError, EmailMessage, EmailSender, LinkChange, NumberCursor, QuoteQuery,
    QuoteRepository, RepositoryError, SalesReferences, SecretService,
};

#[derive(Debug, Clone)]
pub struct QuotePage {
    pub items: Vec<Quote>,
    /// Present when more quotes exist after this page.
    pub next: Option<NumberCursor>,
}

/// A quote just sent, and the link that lets the customer answer it.
#[derive(Debug, Clone)]
pub struct SentQuote {
    pub quote: Quote,
    /// The only time the link exists: the store keeps a digest of its token.
    pub link: String,
    /// Whether the e-mail with the link was sent. It is not a failure when it was not: the person
    /// who sent the quote can pass the link on.
    pub emailed: bool,
}

/// Quotes of a company, and the customer's answer through a link ([ADR 0038]).
pub struct QuoteService<Q, R, E, S> {
    repository: Q,
    references: R,
    emails: E,
    secrets: S,
    /// Where the web app is, so a link in an e-mail leads to it.
    public_url: String,
}

impl<Q, R, E, S> QuoteService<Q, R, E, S>
where
    Q: QuoteRepository,
    R: SalesReferences,
    E: EmailSender,
    S: SecretService,
{
    pub fn new(repository: Q, references: R, emails: E, secrets: S, public_url: &str) -> Self {
        Self {
            repository,
            references,
            emails,
            secrets,
            public_url: public_url.trim_end_matches('/').to_owned(),
        }
    }

    /// Validates the draft and what it points at, and stores the quote with the next number of the
    /// company; the creation is recorded in the audit trail under `actor`, in the same write.
    pub async fn register(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        draft: QuoteDraft,
    ) -> Result<Quote, ApplicationError> {
        let quote = Quote::new(QuoteId::generate(), company_id, draft)?;
        self.check_references(&quote, None).await?;
        let audit = audit_entry(
            &quote,
            actor,
            AuditAction::Created,
            quote_creation_changes(&quote),
        );
        let number = self.repository.save(&quote, &audit).await?;
        Ok(quote.with_number(number))
    }

    /// Replaces the content of a quote that is a draft or was sent; a sent one goes back to a draft
    /// and its link stops working. A draft that changes nothing writes nothing and records nothing.
    pub async fn update(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: QuoteId,
        draft: QuoteDraft,
    ) -> Result<Quote, ApplicationError> {
        let before = self.find(company_id, id).await?;
        let after = before.replaced_with(draft)?;
        // Decided on the quote itself, not on what the trail would say: a change the trail has no field
        // for must not be dropped in silence.
        if after == before {
            return Ok(before);
        }
        self.check_references(&after, Some(&before)).await?;
        let audit = audit_entry(
            &after,
            actor,
            AuditAction::Updated,
            quote_changes(&before, &after),
        );
        self.repository
            .update(&after, before.status(), LinkChange::Revoke, &audit)
            .await
            .map_err(changed_or_missing)?;
        Ok(after)
    }

    /// Marks the quote sent, makes the link the customer answers through and e-mails it to `to`.
    /// A quote that was sent can be sent again: the earlier link stops working.
    pub async fn send(
        &self,
        company_id: CompanyId,
        actor: &Actor,
        id: QuoteId,
        to: &str,
        company_name: &str,
        today: CalendarDate,
    ) -> Result<SentQuote, ApplicationError> {
        let to = Email::parse(to)?;
        let before = self.find(company_id, id).await?;
        let after = before.sent(actor.clone())?;
        // A link to a quote that cannot be approved is no use to anyone.
        if before.is_expired_on(today) {
            return Err(ApplicationError::QuoteExpired);
        }
        let token = self.secrets.new_token();
        let mut changes = quote_changes(&before, &after);
        changes.push(FieldChange {
            field: "link".to_owned(),
            from: None,
            // Not the address: the trail is kept for good and the address is a third party's.
            to: Some("made and sent".to_owned()),
        });
        let audit = audit_entry(&after, actor, AuditAction::Updated, changes);
        self.repository
            .update(
                &after,
                before.status(),
                LinkChange::Set(self.secrets.digest_token(&token)),
                &audit,
            )
            .await
            .map_err(changed_or_missing)?;
        let link = format!("{}/quote/{token}", self.public_url);
        let message = EmailMessage {
            to,
            subject: format!(
                "Orçamento nº {} de {}",
                after.number(),
                one_line(company_name)
            ),
            body: format!(
                "{} enviou um orçamento para você.\n\n\
                 Para ver os itens e aprovar ou recusar, abra o link:\n{link}\n\n\
                 O orçamento vale até {}.\n",
                one_line(company_name),
                after.valid_until()
            ),
        };
        let emailed = self.emails.send(&message).await.is_ok();
        Ok(SentQuote {
            quote: after,
            link,
            emailed,
        })
    }

    /// The quote a link leads to; a link that was never made, or that stopped working, leads to none.
    pub async fn by_link(&self, token: &str) -> Result<Quote, ApplicationError> {
        self.repository
            .find_by_link(&self.secrets.digest_token(token))
            .await?
            .ok_or(ApplicationError::QuoteNotFound)
    }

    /// The customer's answer, through the link: it is recorded in the name of whoever sent the quote,
    /// with what says it was the customer. A quote past its validity cannot be approved.
    pub async fn answer(
        &self,
        token: &str,
        approved: bool,
        today: CalendarDate,
    ) -> Result<Quote, ApplicationError> {
        let before = self.by_link(token).await?;
        let after = before.answered(approved)?;
        // A quote past its validity is declined all the same: only an approval is refused.
        if approved && before.is_expired_on(today) {
            return Err(ApplicationError::QuoteExpired);
        }
        let Some(sent_by) = before.sent_by() else {
            return Err(ApplicationError::QuoteNotFound);
        };
        let mut changes = quote_changes(&before, &after);
        changes.push(FieldChange {
            field: "answered_by".to_owned(),
            from: None,
            to: Some("the customer, through the link".to_owned()),
        });
        let audit = audit_entry(&after, sent_by, AuditAction::Updated, changes);
        self.repository
            .answer(&after, &self.secrets.digest_token(token), &audit)
            .await
            .map_err(changed_or_missing)?;
        Ok(after)
    }

    pub async fn get(&self, company_id: CompanyId, id: QuoteId) -> Result<Quote, ApplicationError> {
        self.find(company_id, id).await
    }

    /// What was recorded about the quote, oldest first.
    pub async fn history(
        &self,
        company_id: CompanyId,
        id: QuoteId,
    ) -> Result<Vec<AuditRecord>, ApplicationError> {
        // Another company's quote answers like an unknown one, never with its trail.
        self.find(company_id, id).await?;
        Ok(self.repository.history(company_id, id).await?)
    }

    pub async fn list(
        &self,
        company_id: CompanyId,
        query: &QuoteQuery,
        after: Option<&NumberCursor>,
        limit: Option<usize>,
    ) -> Result<QuotePage, ApplicationError> {
        let limit = clamp_limit(limit);
        let rows = self
            .repository
            .list_by_company(company_id, query, after, limit + 1)
            .await?;
        let (items, next) = split_page(rows, limit);
        Ok(QuotePage { items, next })
    }

    async fn find(&self, company_id: CompanyId, id: QuoteId) -> Result<Quote, ApplicationError> {
        self.repository
            .find(company_id, id)
            .await?
            .ok_or(ApplicationError::QuoteNotFound)
    }

    /// The customer, the term and the products must be usable in the company, the answer naming the
    /// field. When the quote was already stored, only what the edit **changes** is checked: a product
    /// that was deactivated since does not stop the rest of the quote from being fixed.
    async fn check_references(
        &self,
        quote: &Quote,
        before: Option<&Quote>,
    ) -> Result<(), ApplicationError> {
        let company = quote.company_id();
        if before.is_none_or(|stored| stored.customer_id() != quote.customer_id())
            && !self
                .references
                .customer_exists(company, quote.customer_id())
                .await?
        {
            return Err(ApplicationError::UnusableReference("customer"));
        }
        if let Some(term) = quote.payment_term_id()
            && before.is_none_or(|stored| stored.payment_term_id() != quote.payment_term_id())
            && !self
                .references
                .payment_term_is_usable(company, term)
                .await?
        {
            return Err(ApplicationError::UnusableReference("payment term"));
        }
        for line in quote.lines() {
            let already_there = before.is_some_and(|stored| {
                stored
                    .lines()
                    .iter()
                    .any(|old| old.product_id() == line.product_id())
            });
            if !already_there
                && !self
                    .references
                    .product_is_usable(company, line.product_id())
                    .await?
            {
                return Err(ApplicationError::UnusableReference("product"));
            }
        }
        Ok(())
    }
}

fn changed_or_missing(error: RepositoryError) -> ApplicationError {
    match error {
        RepositoryError::NotFound => ApplicationError::QuoteNotFound,
        RepositoryError::QuoteChanged => ApplicationError::QuoteChanged,
        other => other.into(),
    }
}

/// A name typed by a person goes into a subject line: a line break in it would add headers.
fn one_line(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}
