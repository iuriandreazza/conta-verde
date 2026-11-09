/// What a member of a company is, which decides what they may do there ([ADR 0026]). Permissions
/// are coarse, by area of the product, and nothing here is about a single record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Created the company. Can do everything, and is the only one who can hand the company over.
    Owner,
    Admin,
    Financial,
    Sales,
    Stock,
    /// The company's external accountant: reads everything but who belongs to the company, writes none
    /// of the business data, and is the one role besides the owner and admins that closes a month
    /// ([ADR 0067]).
    Accountant,
    Viewer,
}

/// A part of the product that a permission is about. A route belongs to exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Area {
    /// Customers, suppliers, carriers and sellers.
    Parties,
    /// Products and services.
    Products,
    /// The registers money hangs from: categories, cost centers, bank accounts, payment methods and
    /// payment terms. Entries and settlements join them in Phase 2.
    Finance,
    /// Quotes, sales, contracts, price lists, commissions and their reports ([ADR 0037]).
    Sales,
    /// Purchase orders and their receipts, stock and its movements, counts and reports ([ADR 0052]).
    Purchases,
    /// Closing a month and getting the books out: the period lock, the exports and the schedule of the
    /// monthly package ([ADR 0067]). Everyone who reaches the company reads it; the accountant writes it.
    Accounting,
    /// Issuing fiscal documents: the settings, the documents, their events and the manifestation of the ones received
    /// ([ADR 0073]). Whoever handles the company's money writes it; everyone else reads and downloads.
    Fiscal,
    /// Who is a member of the company, and their invitations.
    Members,
}

/// Looking at an area, or changing it. Writing includes reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Access {
    Read,
    Write,
}

impl Role {
    pub const ALL: [Role; 7] = [
        Self::Owner,
        Self::Admin,
        Self::Financial,
        Self::Sales,
        Self::Stock,
        Self::Accountant,
        Self::Viewer,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Financial => "financial",
            Self::Sales => "sales",
            Self::Stock => "stock",
            Self::Accountant => "accountant",
            Self::Viewer => "viewer",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.as_str() == raw)
    }

    /// How far this role reaches into an area, `None` for not at all. The table of [ADR 0026]:
    ///
    /// | role       | parties | products | finance | sales | purchases | accounting | fiscal | members |
    /// |------------|---------|----------|---------|-------|-----------|------------|--------|---------|
    /// | owner      | write   | write    | write   | write | write     | write      | write  | write   |
    /// | admin      | write   | write    | write   | write | write     | write      | write  | write   |
    /// | financial  | write   | read     | write   | read  | read      | read       | write  | -       |
    /// | sales      | write   | read     | read    | write | read      | read       | read   | -       |
    /// | stock      | read    | write    | read    | read  | write     | read       | read   | -       |
    /// | accountant | read    | read     | read    | read  | read      | write      | read   | -       |
    /// | viewer     | read    | read     | read    | read  | read      | read       | read   | -       |
    pub fn access_to(self, area: Area) -> Option<Access> {
        use Access::{Read, Write};
        match (self, area) {
            (Self::Owner | Self::Admin, _) => Some(Write),
            (_, Area::Members) => None,
            (Self::Financial, Area::Parties | Area::Finance | Area::Fiscal) => Some(Write),
            (Self::Sales, Area::Parties | Area::Sales) => Some(Write),
            (Self::Stock, Area::Products | Area::Purchases) => Some(Write),
            (Self::Accountant, Area::Accounting) => Some(Write),
            _ => Some(Read),
        }
    }

    /// Whether the role may look at (`Read`) or change (`Write`) the area.
    pub fn can(self, area: Area, access: Access) -> bool {
        self.access_to(area) >= Some(access)
    }
}

impl Area {
    pub const ALL: [Area; 8] = [
        Self::Parties,
        Self::Products,
        Self::Finance,
        Self::Sales,
        Self::Purchases,
        Self::Accounting,
        Self::Fiscal,
        Self::Members,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parties => "parties",
            Self::Products => "products",
            Self::Finance => "finance",
            Self::Sales => "sales",
            Self::Purchases => "purchases",
            Self::Accounting => "accounting",
            Self::Fiscal => "fiscal",
            Self::Members => "members",
        }
    }
}

impl Access {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_are_stable_and_round_trip() {
        let keys: Vec<&str> = Role::ALL.iter().map(|role| role.as_str()).collect();
        assert_eq!(
            keys,
            [
                "owner",
                "admin",
                "financial",
                "sales",
                "stock",
                "accountant",
                "viewer"
            ]
        );
        for role in Role::ALL {
            assert_eq!(Role::parse(role.as_str()), Some(role));
        }
        assert_eq!(Role::parse("Owner"), None);
        assert_eq!(Role::parse("root"), None);
    }

    /// Written out in full, apart from the code that computes it: a change to the table is a change
    /// to this test, and so to a decision that [ADR 0026] makes.
    #[test]
    fn the_table_of_permissions() {
        const W: Option<Access> = Some(Access::Write);
        const R: Option<Access> = Some(Access::Read);
        const N: Option<Access> = None;
        // parties, products, finance, sales, purchases, accounting, fiscal, members
        let expected = [
            (Role::Owner, [W, W, W, W, W, W, W, W]),
            (Role::Admin, [W, W, W, W, W, W, W, W]),
            (Role::Financial, [W, R, W, R, R, R, W, N]),
            (Role::Sales, [W, R, R, W, R, R, R, N]),
            (Role::Stock, [R, W, R, R, W, R, R, N]),
            (Role::Accountant, [R, R, R, R, R, W, R, N]),
            (Role::Viewer, [R, R, R, R, R, R, R, N]),
        ];
        for (role, levels) in expected {
            let found = Area::ALL.map(|area| role.access_to(area));
            assert_eq!(found, levels, "{role:?}");
        }
    }

    #[test]
    fn writing_includes_reading_and_no_access_includes_neither() {
        assert!(Role::Sales.can(Area::Parties, Access::Write));
        assert!(Role::Sales.can(Area::Parties, Access::Read));
        assert!(Role::Sales.can(Area::Products, Access::Read));
        assert!(!Role::Sales.can(Area::Products, Access::Write));
        assert!(Role::Sales.can(Area::Sales, Access::Write));
        assert!(!Role::Financial.can(Area::Sales, Access::Write));
        assert!(Role::Stock.can(Area::Purchases, Access::Write));
        assert!(!Role::Sales.can(Area::Purchases, Access::Write));
        assert!(!Role::Viewer.can(Area::Members, Access::Read));
        assert!(!Role::Accountant.can(Area::Members, Access::Write));
    }

    #[test]
    fn the_accountant_writes_nothing_but_the_accounting_area() {
        for area in Area::ALL {
            let writes = Role::Accountant.can(area, Access::Write);
            assert_eq!(writes, area == Area::Accounting, "{area:?}");
        }
        assert!(!Role::Financial.can(Area::Accounting, Access::Write));
    }
}
