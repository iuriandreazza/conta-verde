use uuid::Uuid;

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(Uuid);

        impl $name {
            pub fn generate() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }
    };
}

uuid_id!(ApiKeyId);
uuid_id!(AttachmentId);
uuid_id!(BankAccountId);
uuid_id!(BlobId);
uuid_id!(CategoryId);
uuid_id!(ChargeId);
uuid_id!(CommissionRuleId);
uuid_id!(CompanyId);
uuid_id!(ContractId);
uuid_id!(CostCenterId);
uuid_id!(DocumentMailId);
uuid_id!(EntryId);
uuid_id!(FiscalDocumentId);
uuid_id!(FiscalEventId);
uuid_id!(InventoryCountId);
uuid_id!(InvitationId);
uuid_id!(JobId);
uuid_id!(NotificationId);
uuid_id!(PartyId);
uuid_id!(PaymentEventId);
uuid_id!(PaymentMethodId);
uuid_id!(PaymentTermId);
uuid_id!(PriceListId);
uuid_id!(ReceivedInvoiceId);
uuid_id!(ProductId);
uuid_id!(PurchaseOrderId);
uuid_id!(PurchaseReceiptId);
uuid_id!(QuoteId);
uuid_id!(RecurrenceId);
uuid_id!(SaleId);
uuid_id!(SaleReturnId);
uuid_id!(SettlementId);
uuid_id!(StatementLineId);
uuid_id!(StockMovementId);
uuid_id!(TransferId);
uuid_id!(UserId);

impl From<AttachmentId> for BlobId {
    /// A file of an attachment is kept under the attachment's own id, so it stays where it always was.
    fn from(id: AttachmentId) -> Self {
        Self(id.as_uuid())
    }
}
