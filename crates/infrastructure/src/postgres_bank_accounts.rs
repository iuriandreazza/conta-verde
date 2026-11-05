use conta_verde_application::{
    BankAccountQuery, BankAccountRepository, NameCursor, RepositoryError, ensure_audit_describes,
};
use conta_verde_domain::{
    AuditEntity, AuditEntry, AuditRecord, BankAccount, BankAccountDraft, BankAccountId,
    BankAccountKind, CompanyId,
};
use sqlx::postgres::PgRow;
use sqlx::{PgPool, Postgres, QueryBuilder, Row};

use crate::postgres::{push_name_keyset, storage_error};
use crate::postgres_audit;

const NAME_UNIQUE_INDEX: &str = "bank_accounts_company_name_key";
const IDENTITY_UNIQUE_INDEX: &str = "bank_accounts_company_identity_key";

const SELECT_BANK_ACCOUNTS: &str =
    "SELECT id, company_id, kind, name, bank_code, branch, number, active FROM bank_accounts";

pub struct PostgresBankAccountRepository {
    pool: PgPool,
}

impl PostgresBankAccountRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn write_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(db) => match db.constraint() {
            Some(NAME_UNIQUE_INDEX) => RepositoryError::DuplicateName,
            Some(IDENTITY_UNIQUE_INDEX) => RepositoryError::DuplicateBankAccount,
            _ => storage_error(error),
        },
        _ => storage_error(error),
    }
}

impl BankAccountRepository for PostgresBankAccountRepository {
    async fn save(&self, account: &BankAccount, audit: &AuditEntry) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, account)?;
        // One transaction: if the account is refused or the audit row cannot be written, the
        // rollback leaves neither behind.
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        sqlx::query(
            "INSERT INTO bank_accounts (id, company_id, kind, name, bank_code, branch, number, active) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(account.id().as_uuid())
        .bind(account.company_id().as_uuid())
        .bind(account.kind().as_str())
        .bind(account.name())
        .bind(account.bank_code())
        .bind(account.branch())
        .bind(account.number())
        .bind(account.is_active())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn find(
        &self,
        company_id: CompanyId,
        id: BankAccountId,
    ) -> Result<Option<BankAccount>, RepositoryError> {
        let row = sqlx::query(&format!(
            "{SELECT_BANK_ACCOUNTS} WHERE company_id = $1 AND id = $2"
        ))
        .bind(company_id.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(storage_error)?;
        row.as_ref().map(account_from_row).transpose()
    }

    async fn update(
        &self,
        account: &BankAccount,
        audit: &AuditEntry,
    ) -> Result<(), RepositoryError> {
        ensure_audit_describes(audit, account)?;
        let mut tx = self.pool.begin().await.map_err(storage_error)?;
        let result = sqlx::query(
            "UPDATE bank_accounts SET name = $3, bank_code = $4, branch = $5, number = $6, active = $7 \
             WHERE id = $1 AND company_id = $2 AND kind = $8",
        )
        .bind(account.id().as_uuid())
        .bind(account.company_id().as_uuid())
        .bind(account.name())
        .bind(account.bank_code())
        .bind(account.branch())
        .bind(account.number())
        .bind(account.is_active())
        .bind(account.kind().as_str())
        .execute(&mut *tx)
        .await
        .map_err(write_error)?;
        // Matching no row means the account is not in that company, or not of that kind (the kind
        // is fixed at registration). Report it, do not pretend. The transaction is dropped, so no
        // audit row is left for a write that did not happen.
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        postgres_audit::insert(&mut tx, audit).await?;
        tx.commit().await.map_err(storage_error)
    }

    async fn history(
        &self,
        company_id: CompanyId,
        id: BankAccountId,
    ) -> Result<Vec<AuditRecord>, RepositoryError> {
        postgres_audit::history(
            &self.pool,
            company_id,
            AuditEntity::BankAccount,
            id.as_uuid(),
        )
        .await
    }

    async fn list_by_company(
        &self,
        company_id: CompanyId,
        query: &BankAccountQuery,
        after: Option<&NameCursor>,
        limit: usize,
    ) -> Result<Vec<BankAccount>, RepositoryError> {
        // Every value is bound, never spliced into the text: the pieces pushed as text are fixed.
        let mut sql = QueryBuilder::<Postgres>::new(SELECT_BANK_ACCOUNTS);
        sql.push(" WHERE company_id = ")
            .push_bind(company_id.as_uuid());
        if let Some(kind) = query.kind() {
            sql.push(" AND kind = ").push_bind(kind.as_str());
        }
        if let Some(active) = query.active() {
            sql.push(" AND active = ").push_bind(active);
        }
        if let Some(text) = query.search() {
            // `strpos` instead of `LIKE` so that `%` and `_` in the text mean themselves.
            sql.push(" AND (strpos(lower(name), lower(")
                .push_bind(text)
                .push(")) > 0");
            if let Some(needle) = query.number_search() {
                sql.push(" OR strpos(coalesce(number, ''), ")
                    .push_bind(needle)
                    .push(") > 0");
            }
            sql.push(")");
        }
        push_name_keyset(&mut sql, query.sort(), after, limit);

        let rows = sql
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;
        rows.iter().map(account_from_row).collect()
    }
}

fn account_from_row(row: &PgRow) -> Result<BankAccount, RepositoryError> {
    let id = BankAccountId::from_uuid(row.try_get("id").map_err(storage_error)?);
    let company_id = CompanyId::from_uuid(row.try_get("company_id").map_err(storage_error)?);
    let kind: String = row.try_get("kind").map_err(storage_error)?;
    let kind = BankAccountKind::parse(&kind)
        .ok_or_else(|| RepositoryError::Storage(format!("unknown bank account kind `{kind}`")))?;
    let draft = BankAccountDraft {
        kind,
        name: row.try_get("name").map_err(storage_error)?,
        bank_code: row.try_get("bank_code").map_err(storage_error)?,
        branch: row.try_get("branch").map_err(storage_error)?,
        number: row.try_get("number").map_err(storage_error)?,
        active: row.try_get("active").map_err(storage_error)?,
    };
    BankAccount::new(id, company_id, draft).map_err(|error| {
        RepositoryError::Storage(format!("stored bank account {id:?} is invalid: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use conta_verde_application::BankAccountService;

    use super::*;
    use crate::contract;

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn stores_bank_accounts_with_their_trail() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let repository = PostgresBankAccountRepository::new(pool);
        contract::stores_bank_accounts_with_their_trail(&repository, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn edits_bank_accounts_and_records_only_changes() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = BankAccountService::new(PostgresBankAccountRepository::new(pool));
        contract::edits_bank_accounts_and_records_only_changes(&service, company, other_company)
            .await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn filters_and_sorts_bank_accounts() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let other_company = contract::create_company(&pool).await;
        let service = BankAccountService::new(PostgresBankAccountRepository::new(pool));
        contract::filters_and_sorts_bank_accounts(&service, company, other_company).await;
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL"]
    async fn the_database_refuses_what_the_domain_would() {
        let pool = contract::test_pool().await;
        let company = contract::create_company(&pool).await;
        let refused_by = |result: Result<_, sqlx::Error>| match result {
            Err(sqlx::Error::Database(db)) => db.constraint().map(str::to_owned),
            _ => None,
        };
        let insert = |columns: &'static str, values: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query(&format!(
                    "INSERT INTO bank_accounts (id, company_id, active, {columns}) VALUES ($1, $2, true, {values})"
                ))
                .bind(uuid::Uuid::new_v4())
                .bind(company.as_uuid())
                .execute(&pool)
                .await
            }
        };
        let case = |columns: &'static str, values: &'static str| async move {
            refused_by(insert(columns, values).await)
        };
        let cases: [(&str, &str, &str); 10] = [
            ("kind, name", "'wallet', 'x'", "bank_accounts_kind_valid"),
            (
                "kind, name",
                "'checking', ''",
                "bank_accounts_name_is_short",
            ),
            (
                "kind, name, bank_code",
                "'checking', 'x', '34'",
                "bank_accounts_bank_code_shape",
            ),
            (
                "kind, name, bank_code",
                "'checking', 'x', '1234567'",
                "bank_accounts_bank_code_shape",
            ),
            (
                "kind, name, branch",
                "'checking', 'x', '12-3'",
                "bank_accounts_branch_shape",
            ),
            (
                "kind, name, branch",
                "'checking', 'x', 'ab12'",
                "bank_accounts_branch_shape",
            ),
            (
                "kind, name, number",
                "'checking', 'x', '1 2'",
                "bank_accounts_number_shape",
            ),
            (
                "kind, name, number",
                "'cash', 'x', '123'",
                "bank_accounts_cash_has_no_bank",
            ),
            (
                "kind, name, number",
                "'card', 'x', '4111111111111111'",
                "bank_accounts_card_has_no_number",
            ),
            (
                "kind, name",
                "'checking', ' x '",
                "bank_accounts_name_is_short",
            ),
        ];
        for (columns, values, constraint) in cases {
            assert_eq!(
                case(columns, values).await.as_deref(),
                Some(constraint),
                "{values}"
            );
        }
        // And a valid one is stored, so the cases above were refused for their own reason.
        assert!(
            insert(
                "kind, name, bank_code, branch, number",
                "'checking', 'ok', '341', '1234', '56789X'"
            )
            .await
            .is_ok()
        );
    }
}
