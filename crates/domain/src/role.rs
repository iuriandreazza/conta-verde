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
    /// The company's external accountant: reads everything; period locks and exports come with
    /// Phase 6 ([ADR 0013]).
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
    /// | role       | parties | products | finance | members |
    /// |------------|---------|----------|---------|---------|
    /// | owner      | write   | write    | write   | write   |
    /// | admin      | write   | write    | write   | write   |
    /// | financial  | write   | read     | write   | -       |
    /// | sales      | write   | read     | read    | -       |
    /// | stock      | read    | write    | read    | -       |
    /// | accountant | read    | read     | read    | -       |
    /// | viewer     | read    | read     | read    | -       |
    pub fn access_to(self, area: Area) -> Option<Access> {
        use Access::{Read, Write};
        match (self, area) {
            (Self::Owner | Self::Admin, _) => Some(Write),
            (_, Area::Members) => None,
            (Self::Financial, Area::Parties | Area::Finance) => Some(Write),
            (Self::Sales, Area::Parties) => Some(Write),
            (Self::Stock, Area::Products) => Some(Write),
            _ => Some(Read),
        }
    }

    /// Whether the role may look at (`Read`) or change (`Write`) the area.
    pub fn can(self, area: Area, access: Access) -> bool {
        self.access_to(area) >= Some(access)
    }
}

impl Area {
    pub const ALL: [Area; 4] = [Self::Parties, Self::Products, Self::Finance, Self::Members];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parties => "parties",
            Self::Products => "products",
            Self::Finance => "finance",
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
        use Access::{Read, Write};
        // parties, products, finance, members
        let expected = [
            (
                Role::Owner,
                [Some(Write), Some(Write), Some(Write), Some(Write)],
            ),
            (
                Role::Admin,
                [Some(Write), Some(Write), Some(Write), Some(Write)],
            ),
            (
                Role::Financial,
                [Some(Write), Some(Read), Some(Write), None],
            ),
            (Role::Sales, [Some(Write), Some(Read), Some(Read), None]),
            (Role::Stock, [Some(Read), Some(Write), Some(Read), None]),
            (Role::Accountant, [Some(Read), Some(Read), Some(Read), None]),
            (Role::Viewer, [Some(Read), Some(Read), Some(Read), None]),
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
        assert!(!Role::Viewer.can(Area::Members, Access::Read));
        assert!(!Role::Accountant.can(Area::Members, Access::Write));
    }
}
