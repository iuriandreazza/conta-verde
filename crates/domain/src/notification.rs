use crate::{CompanyId, DunningChannel, EntryId, NotificationId};

/// How many times a reminder is tried at most: a failure is tried again on the next passes.
pub const MAX_NOTIFICATION_ATTEMPTS: u8 = 3;
const MAX_NOTE_CHARS: usize = 200;

/// Where a reminder is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationStatus {
    /// Claimed and being sent. One that stays so (the server stopped meanwhile) is **not** sent again: a
    /// customer who gets a reminder twice is worse off than one who misses it.
    Pending,
    Sent,
    Failed,
}

impl NotificationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sent => "sent",
            Self::Failed => "failed",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pending" => Some(Self::Pending),
            "sent" => Some(Self::Sent),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// One reminder to one customer about one installment, by one channel, for one rule ([ADR 0048]). There is
/// at most one per installment, rule days and channel (a unique key in the store), which is what makes the
/// hourly pass safe to repeat. A log, not a register: it is not audited, and only moves from pending to
/// sent or failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    id: NotificationId,
    company_id: CompanyId,
    entry_id: EntryId,
    installment: u16,
    days: i16,
    channel: DunningChannel,
    /// The address or phone it went to, as the customer's contact gave it.
    recipient: String,
    status: NotificationStatus,
    attempts: u8,
    note: Option<String>,
}

impl Notification {
    /// A reminder about to be sent: pending, on its first attempt.
    pub fn new(
        id: NotificationId,
        company_id: CompanyId,
        entry_id: EntryId,
        installment: u16,
        days: i16,
        channel: DunningChannel,
        recipient: &str,
    ) -> Self {
        Self {
            id,
            company_id,
            entry_id,
            installment,
            days,
            channel,
            recipient: recipient
                .chars()
                .filter(|c| !c.is_control())
                .take(200)
                .collect(),
            status: NotificationStatus::Pending,
            attempts: 1,
            note: None,
        }
    }

    /// The same reminder in another state: what storage hands back, and what a try makes.
    pub fn with_state(
        mut self,
        status: NotificationStatus,
        attempts: u8,
        note: Option<&str>,
    ) -> Self {
        self.status = status;
        self.attempts = attempts;
        self.note = note
            .map(|text| {
                text.chars()
                    .filter(|c| !c.is_control())
                    .take(MAX_NOTE_CHARS)
                    .collect::<String>()
            })
            .filter(|text| !text.is_empty());
        self
    }

    pub fn id(&self) -> NotificationId {
        self.id
    }

    pub fn company_id(&self) -> CompanyId {
        self.company_id
    }

    pub fn entry_id(&self) -> EntryId {
        self.entry_id
    }

    pub fn installment(&self) -> u16 {
        self.installment
    }

    pub fn days(&self) -> i16 {
        self.days
    }

    pub fn channel(&self) -> DunningChannel {
        self.channel
    }

    pub fn recipient(&self) -> &str {
        &self.recipient
    }

    pub fn status(&self) -> NotificationStatus {
        self.status
    }

    pub fn attempts(&self) -> u8 {
        self.attempts
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }
}
