use crate::{
    AccessKey, Amount, BlobId, CalendarDate, CompanyId, DomainError, FiscalDocumentId,
    FiscalEnvironment, SaleId, TaxRegime, UserId,
};

const MAX_TEXT_CHARS: usize = 150;
const MAX_REFUSAL_CODE_CHARS: usize = 50;
const MAX_REFUSAL_MESSAGE_CHARS: usize = 500;
const MAX_NOTE_CHARS: usize = 500;
/// The number of a document has nine digits in its access key.
const MAX_NUMBER: u32 = 999_999_999;

/// What kind of fiscal document it is. NFS-e and NFC-e join with their slices ([ADR 0073]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiscalDocumentKind {
    Nfe,
}

impl FiscalDocumentKind {
    /// The key the API sends and the database stores; it does not change.
    pub fn key(self) -> &'static str {
        match self {
            Self::Nfe => "nfe",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "nfe" => Some(Self::Nfe),
            _ => None,
        }
    }
}

/// Where a document is in its life ([ADR 0073], [ADR 0076]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiscalDocumentStatus {
    /// Written, nothing sent.
    Draft,
    /// Sent to the gateway, not answered yet.
    Processing,
    Authorized,
    /// Issued while the authority was out, still waiting to be authorized.
    Contingency,
    Rejected,
    /// The authority denied it for a reason about the issuer or the recipient: final.
    Denied,
    Cancelled,
}

impl FiscalDocumentStatus {
    pub fn key(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Processing => "processing",
            Self::Authorized => "authorized",
            Self::Contingency => "contingency",
            Self::Rejected => "rejected",
            Self::Denied => "denied",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "draft" => Some(Self::Draft),
            "processing" => Some(Self::Processing),
            "authorized" => Some(Self::Authorized),
            "contingency" => Some(Self::Contingency),
            "rejected" => Some(Self::Rejected),
            "denied" => Some(Self::Denied),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Whether the document still holds its sale: only one such document exists per sale and kind. A rejected,
    /// denied or cancelled one frees the sale for a new document.
    pub fn is_live(self) -> bool {
        matches!(
            self,
            Self::Draft | Self::Processing | Self::Authorized | Self::Contingency
        )
    }

    /// Whether the answer of the gateway is still to come, so the document is asked about.
    pub fn is_pending(self) -> bool {
        matches!(self, Self::Processing | Self::Contingency)
    }
}

/// What a document was made from ([ADR 0076]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiscalOrigin {
    Sale(SaleId),
}

/// Why the authority or the gateway refused, as the person must read it to fix the document: the code and the text,
/// without control characters and cut to a length a row can hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiscalRefusal {
    code: String,
    message: String,
}

impl FiscalRefusal {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: clean(code, MAX_REFUSAL_CODE_CHARS),
            message: clean(message, MAX_REFUSAL_MESSAGE_CHARS),
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

/// What the authority gave an authorized document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiscalAuthorization {
    access_key: AccessKey,
    number: u32,
    series: u16,
    protocol: Option<String>,
}

impl FiscalAuthorization {
    pub fn new(
        access_key: AccessKey,
        number: u32,
        series: u16,
        protocol: Option<&str>,
    ) -> Result<Self, DomainError> {
        if number == 0 || number > MAX_NUMBER || series == 0 || series > 999 {
            return Err(DomainError::InvalidFiscalDocument("authorization"));
        }
        let protocol = protocol
            .map(|raw| clean(raw, MAX_REFUSAL_CODE_CHARS))
            .filter(|protocol| !protocol.is_empty());
        Ok(Self {
            access_key,
            number,
            series,
            protocol,
        })
    }

    pub fn access_key(&self) -> &AccessKey {
        &self.access_key
    }

    pub fn number(&self) -> u32 {
        self.number
    }

    pub fn series(&self) -> u16 {
        self.series
    }

    pub fn protocol(&self) -> Option<&str> {
        self.protocol.as_deref()
    }
}

/// Unvalidated input for a new document: what the issue found when it validated the sale and the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiscalDocumentDraft {
    pub origin: FiscalOrigin,
    pub issuer_cnpj: String,
    pub issuer_name: String,
    pub regime: TaxRegime,
    pub environment: FiscalEnvironment,
    pub recipient_name: String,
    pub recipient_document: Option<String>,
    pub nature_code: String,
    pub total: Amount,
    /// The moment of issue in Unix seconds: the caller's clock.
    pub issued_at_unix: i64,
    /// The day of that moment on the issuer's clock, which the period lock judges.
    pub issued_on: CalendarDate,
    pub submitted_by: UserId,
    /// The name of that person at the time: history must keep reading the same after they are renamed.
    pub submitted_by_name: String,
}

/// A fiscal document: an NF-e on its way to, or past, the authority ([ADR 0073], [ADR 0076]).
///
/// What it keeps is what a person must read years later; the lines are in the XML the authority signed. Its **content**
/// (origin, issuer, recipient, nature, total, moment) is fixed once it is authorized: only its status and what the
/// authority said change from there. Our id is the gateway's reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiscalDocument {
    id: FiscalDocumentId,
    company_id: CompanyId,
    kind: FiscalDocumentKind,
    content: FiscalDocumentDraft,
    status: FiscalDocumentStatus,
    refusal: Option<FiscalRefusal>,
    authorization: Option<FiscalAuthorization>,
    note: Option<String>,
}

impl FiscalDocument {
    pub fn new(
        id: FiscalDocumentId,
        company_id: CompanyId,
        draft: FiscalDocumentDraft,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id,
            company_id,
            kind: FiscalDocumentKind::Nfe,
            content: validated(draft)?,
            status: FiscalDocumentStatus::Draft,
            refusal: None,
            authorization: None,
            note: None,
        })
    }

    /// The document as it was stored.
    pub fn restored(
        mut self,
        status: FiscalDocumentStatus,
        refusal: Option<FiscalRefusal>,
        authorization: Option<FiscalAuthorization>,
        note: Option<String>,
    ) -> Self {
        self.status = status;
        self.refusal = refusal;
        self.authorization = authorization;
        self.note = note;
        self
    }

    /// The same document with the content of a new attempt: only a draft or a rejected one is built again, because
    /// from authorization on the content never changes.
    pub fn rebuilt_with(&self, draft: FiscalDocumentDraft) -> Result<Self, DomainError> {
        if !matches!(
            self.status,
            FiscalDocumentStatus::Draft | FiscalDocumentStatus::Rejected
        ) {
            return Err(DomainError::InvalidFiscalTransition);
        }
        Ok(Self {
            content: validated(draft)?,
            refusal: None,
            ..self.clone()
        })
    }

    /// Sent to the gateway: a draft, or a rejected one sent again under the same reference.
    pub fn submitted(&self) -> Result<Self, DomainError> {
        self.moved(
            &[FiscalDocumentStatus::Draft, FiscalDocumentStatus::Rejected],
            FiscalDocumentStatus::Processing,
        )
        .map(|document| Self {
            refusal: None,
            ..document
        })
    }

    pub fn in_contingency(&self) -> Result<Self, DomainError> {
        self.moved(
            &[FiscalDocumentStatus::Processing],
            FiscalDocumentStatus::Contingency,
        )
    }

    /// The authority authorized it. `note` says why it was written the way it was (late, in a closed month).
    pub fn authorized(
        &self,
        authorization: FiscalAuthorization,
        note: Option<&str>,
    ) -> Result<Self, DomainError> {
        let mut document = self.moved(
            &[
                FiscalDocumentStatus::Processing,
                FiscalDocumentStatus::Contingency,
            ],
            FiscalDocumentStatus::Authorized,
        )?;
        document.authorization = Some(authorization);
        document.refusal = None;
        document.note = note
            .map(|raw| clean(raw, MAX_NOTE_CHARS))
            .filter(|note| !note.is_empty());
        Ok(document)
    }

    pub fn rejected(&self, refusal: FiscalRefusal) -> Result<Self, DomainError> {
        let mut document = self.moved(
            &[
                FiscalDocumentStatus::Processing,
                FiscalDocumentStatus::Contingency,
            ],
            FiscalDocumentStatus::Rejected,
        )?;
        document.refusal = Some(refusal);
        Ok(document)
    }

    pub fn denied(&self, refusal: FiscalRefusal) -> Result<Self, DomainError> {
        let mut document = self.moved(
            &[
                FiscalDocumentStatus::Processing,
                FiscalDocumentStatus::Contingency,
            ],
            FiscalDocumentStatus::Denied,
        )?;
        document.refusal = Some(refusal);
        Ok(document)
    }

    pub fn cancelled(&self) -> Result<Self, DomainError> {
        self.moved(
            &[FiscalDocumentStatus::Authorized],
            FiscalDocumentStatus::Cancelled,
        )
    }

    fn moved(
        &self,
        from: &[FiscalDocumentStatus],
        to: FiscalDocumentStatus,
    ) -> Result<Self, DomainError> {
        if !from.contains(&self.status) {
            return Err(DomainError::InvalidFiscalTransition);
        }
        Ok(Self {
            status: to,
            ..self.clone()
        })
    }

    pub fn id(&self) -> FiscalDocumentId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn kind(&self) -> FiscalDocumentKind {
        self.kind
    }

    pub fn origin(&self) -> FiscalOrigin {
        self.content.origin
    }

    pub fn issuer_cnpj(&self) -> &str {
        &self.content.issuer_cnpj
    }

    pub fn issuer_name(&self) -> &str {
        &self.content.issuer_name
    }

    pub fn regime(&self) -> TaxRegime {
        self.content.regime
    }

    pub fn environment(&self) -> FiscalEnvironment {
        self.content.environment
    }

    pub fn recipient_name(&self) -> &str {
        &self.content.recipient_name
    }

    pub fn recipient_document(&self) -> Option<&str> {
        self.content.recipient_document.as_deref()
    }

    pub fn nature_code(&self) -> &str {
        &self.content.nature_code
    }

    pub fn total(&self) -> Amount {
        self.content.total
    }

    pub fn issued_at_unix(&self) -> i64 {
        self.content.issued_at_unix
    }

    pub fn issued_on(&self) -> CalendarDate {
        self.content.issued_on
    }

    pub fn submitted_by(&self) -> UserId {
        self.content.submitted_by
    }

    pub fn submitted_by_name(&self) -> &str {
        &self.content.submitted_by_name
    }

    pub fn status(&self) -> FiscalDocumentStatus {
        self.status
    }

    pub fn refusal(&self) -> Option<&FiscalRefusal> {
        self.refusal.as_ref()
    }

    pub fn authorization(&self) -> Option<&FiscalAuthorization> {
        self.authorization.as_ref()
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Whether the content differs from another's: what an adapter refuses to write once authorized.
    pub fn content_differs_from(&self, other: &Self) -> bool {
        self.content != other.content || self.kind != other.kind
    }
}

fn validated(mut draft: FiscalDocumentDraft) -> Result<FiscalDocumentDraft, DomainError> {
    let text = |raw: &str, field: &'static str| -> Result<String, DomainError> {
        let cleaned = raw.trim();
        let valid = !cleaned.is_empty()
            && cleaned.chars().count() <= MAX_TEXT_CHARS
            && !cleaned.chars().any(char::is_control);
        if valid {
            Ok(cleaned.to_owned())
        } else {
            Err(DomainError::InvalidFiscalDocument(field))
        }
    };
    draft.issuer_name = text(&draft.issuer_name, "issuer_name")?;
    draft.recipient_name = text(&draft.recipient_name, "recipient_name")?;
    draft.nature_code = text(&draft.nature_code, "nature_code")?;
    draft.submitted_by_name = text(&draft.submitted_by_name, "submitted_by_name")?;
    let cnpj_valid = draft.issuer_cnpj.len() == 14
        && draft
            .issuer_cnpj
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_uppercase());
    if !cnpj_valid {
        return Err(DomainError::InvalidFiscalDocument("issuer_cnpj"));
    }
    if let Some(document) = &draft.recipient_document {
        let valid = matches!(document.len(), 11 | 14)
            && document
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte.is_ascii_uppercase());
        if !valid {
            return Err(DomainError::InvalidFiscalDocument("recipient_document"));
        }
    }
    if draft.total.as_decimal().scale() > 2 || draft.total.as_decimal().is_zero() {
        return Err(DomainError::InvalidFiscalDocument("total"));
    }
    Ok(draft)
}

/// Text from outside, safe to store and show: no control characters, cut to `max` characters.
fn clean(raw: &str, max: usize) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// What a stored file of a document holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiscalFileKind {
    /// The authorized document's XML.
    Xml,
    /// The printed form (the DANFE).
    Pdf,
    /// The XML of the cancellation.
    CancellationXml,
}

impl FiscalFileKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Xml => "xml",
            Self::Pdf => "pdf",
            Self::CancellationXml => "cancellation_xml",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "xml" => Some(Self::Xml),
            "pdf" => Some(Self::Pdf),
            "cancellation_xml" => Some(Self::CancellationXml),
            _ => None,
        }
    }

    /// The type the file is served as.
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Xml | Self::CancellationXml => "application/xml",
            Self::Pdf => "application/pdf",
        }
    }
}

/// A file of a document: the record that says whose the bytes in the blob store are ([ADR 0076]). Never edited or
/// deleted: a fiscal file is the record of an act.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiscalFile {
    blob_id: BlobId,
    company_id: CompanyId,
    document_id: FiscalDocumentId,
    kind: FiscalFileKind,
    size: u64,
}

impl FiscalFile {
    pub fn new(
        blob_id: BlobId,
        company_id: CompanyId,
        document_id: FiscalDocumentId,
        kind: FiscalFileKind,
        size: u64,
    ) -> Result<Self, DomainError> {
        if size == 0 {
            return Err(DomainError::InvalidFiscalDocument("file"));
        }
        Ok(Self {
            blob_id,
            company_id,
            document_id,
            kind,
            size,
        })
    }

    pub fn blob_id(&self) -> BlobId {
        self.blob_id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn document_id(&self) -> FiscalDocumentId {
        self.document_id
    }

    pub fn kind(&self) -> FiscalFileKind {
        self.kind
    }

    pub fn size(&self) -> u64 {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> FiscalDocumentDraft {
        FiscalDocumentDraft {
            origin: FiscalOrigin::Sale(SaleId::generate()),
            issuer_cnpj: "11222333000181".to_owned(),
            issuer_name: "Acme Ltda".to_owned(),
            regime: TaxRegime::SimplesNacional,
            environment: FiscalEnvironment::Homologation,
            recipient_name: "Cliente".to_owned(),
            recipient_document: Some("12345678909".to_owned()),
            nature_code: "venda".to_owned(),
            total: Amount::parse("100.50").unwrap(),
            issued_at_unix: 1_700_000_000,
            issued_on: CalendarDate::parse("2026-11-10").unwrap(),
            submitted_by: UserId::generate(),
            submitted_by_name: "Maria".to_owned(),
        }
    }

    fn document() -> FiscalDocument {
        FiscalDocument::new(FiscalDocumentId::generate(), CompanyId::generate(), draft()).unwrap()
    }

    fn authorization() -> FiscalAuthorization {
        let key = AccessKey::from_parts(
            35,
            CalendarDate::parse("2026-11-10").unwrap(),
            "11222333000181",
            55,
            1,
            7,
            1,
            12345678,
        );
        FiscalAuthorization::new(key.unwrap(), 7, 1, Some(" 135260000000001 ")).unwrap()
    }

    #[test]
    fn a_new_document_is_a_draft() {
        let document = document();
        assert_eq!(document.status(), FiscalDocumentStatus::Draft);
        assert!(document.status().is_live());
        assert!(!document.status().is_pending());
    }

    #[test]
    fn follows_the_path_to_authorization_and_cancellation() {
        let authorized = document()
            .submitted()
            .unwrap()
            .authorized(authorization(), None)
            .unwrap();
        assert_eq!(authorized.status(), FiscalDocumentStatus::Authorized);
        assert_eq!(authorized.authorization().unwrap().number(), 7);
        assert_eq!(
            authorized.authorization().unwrap().protocol(),
            Some("135260000000001")
        );
        let cancelled = authorized.cancelled().unwrap();
        assert_eq!(cancelled.status(), FiscalDocumentStatus::Cancelled);
        assert!(!cancelled.status().is_live());
    }

    #[test]
    fn contingency_is_still_waiting_and_can_authorize() {
        let waiting = document().submitted().unwrap().in_contingency().unwrap();
        assert!(waiting.status().is_pending());
        let authorized = waiting
            .authorized(authorization(), Some("authorized late"))
            .unwrap();
        assert_eq!(authorized.note(), Some("authorized late"));
    }

    #[test]
    fn a_rejected_document_is_sent_again_and_frees_its_sale() {
        let rejected = document()
            .submitted()
            .unwrap()
            .rejected(FiscalRefusal::new("225", "Falha no Schema XML"))
            .unwrap();
        assert!(!rejected.status().is_live());
        assert_eq!(rejected.refusal().unwrap().code(), "225");
        let again = rejected.submitted().unwrap();
        assert_eq!(again.status(), FiscalDocumentStatus::Processing);
        assert!(again.refusal().is_none());
    }

    #[test]
    fn refuses_every_move_the_states_do_not_allow() {
        let draft = document();
        assert!(draft.authorized(authorization(), None).is_err());
        assert!(draft.cancelled().is_err());
        assert!(draft.in_contingency().is_err());
        let processing = draft.submitted().unwrap();
        assert!(processing.submitted().is_err());
        assert!(processing.cancelled().is_err());
        let denied = processing
            .denied(FiscalRefusal::new("301", "Irregular"))
            .unwrap();
        for result in [
            denied.submitted(),
            denied.cancelled(),
            denied.authorized(authorization(), None),
        ] {
            assert_eq!(result, Err(DomainError::InvalidFiscalTransition));
        }
        let cancelled = processing
            .authorized(authorization(), None)
            .unwrap()
            .cancelled()
            .unwrap();
        assert!(cancelled.cancelled().is_err());
        assert!(cancelled.submitted().is_err());
    }

    #[test]
    fn only_a_draft_or_a_rejected_document_is_built_again() {
        let mut changed = draft();
        changed.total = Amount::parse("200").unwrap();
        let draft_again = document().rebuilt_with(changed.clone()).unwrap();
        assert_eq!(draft_again.total(), Amount::parse("200").unwrap());
        let authorized = document()
            .submitted()
            .unwrap()
            .authorized(authorization(), None)
            .unwrap();
        assert_eq!(
            authorized.rebuilt_with(changed),
            Err(DomainError::InvalidFiscalTransition)
        );
    }

    #[test]
    fn a_status_change_is_not_a_content_change() {
        let before = document();
        let after = before.submitted().unwrap();
        assert!(!before.content_differs_from(&after));
        let mut changed = draft();
        changed.nature_code = "outra".to_owned();
        assert!(before.content_differs_from(&before.rebuilt_with(changed).unwrap()));
    }

    #[test]
    fn refuses_what_a_row_could_not_hold() {
        let mut bad = draft();
        bad.issuer_cnpj = "123".to_owned();
        assert!(
            FiscalDocument::new(FiscalDocumentId::generate(), CompanyId::generate(), bad).is_err()
        );
        let mut bad = draft();
        bad.recipient_name = " ".to_owned();
        assert!(
            FiscalDocument::new(FiscalDocumentId::generate(), CompanyId::generate(), bad).is_err()
        );
        let mut bad = draft();
        bad.total = Amount::parse("0").unwrap();
        assert!(
            FiscalDocument::new(FiscalDocumentId::generate(), CompanyId::generate(), bad).is_err()
        );
        let mut bad = draft();
        bad.recipient_document = Some("123".to_owned());
        assert!(
            FiscalDocument::new(FiscalDocumentId::generate(), CompanyId::generate(), bad).is_err()
        );
    }

    #[test]
    fn an_authorization_fits_the_nine_digits_of_a_number_and_a_note_is_never_empty() {
        let key = authorization().access_key().clone();
        assert!(FiscalAuthorization::new(key.clone(), 1_000_000_000, 1, None).is_err());
        assert!(FiscalAuthorization::new(key, 999_999_999, 1, None).is_ok());
        let authorized = document()
            .submitted()
            .unwrap()
            .authorized(authorization(), Some(" \u{7} "))
            .unwrap();
        assert_eq!(authorized.note(), None);
    }

    #[test]
    fn a_refusal_is_one_clean_line_of_bounded_length() {
        let refusal = FiscalRefusal::new("2\n25", &format!("a\u{7}{}", "b".repeat(900)));
        assert_eq!(refusal.code(), "225");
        assert_eq!(refusal.message().chars().count(), 500);
        assert!(!refusal.message().contains('\u{7}'));
    }

    #[test]
    fn a_file_has_bytes_and_a_blob_can_be_an_attachments_id() {
        let attachment = crate::AttachmentId::generate();
        assert_eq!(BlobId::from(attachment).as_uuid(), attachment.as_uuid());
        assert!(
            FiscalFile::new(
                BlobId::generate(),
                CompanyId::generate(),
                FiscalDocumentId::generate(),
                FiscalFileKind::Xml,
                0
            )
            .is_err()
        );
    }
}
