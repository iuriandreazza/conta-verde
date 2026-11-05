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

uuid_id!(CategoryId);
uuid_id!(CompanyId);
uuid_id!(CostCenterId);
uuid_id!(PartyId);
uuid_id!(ProductId);
uuid_id!(UserId);
