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

uuid_id!(BankAccountId);
uuid_id!(CategoryId);
uuid_id!(CompanyId);
uuid_id!(CostCenterId);
uuid_id!(EntryId);
uuid_id!(InvitationId);
uuid_id!(PartyId);
uuid_id!(PaymentMethodId);
uuid_id!(PaymentTermId);
uuid_id!(ProductId);
uuid_id!(RecurrenceId);
uuid_id!(SettlementId);
uuid_id!(StatementLineId);
uuid_id!(TransferId);
uuid_id!(UserId);
