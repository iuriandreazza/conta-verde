//! The second barrier ([ADR 0093](../../../docs/adr/0093-row-level-security.md)): what the database itself refuses when the
//! application's own company check is skipped or wrong.

use conta_verde_application::PartyRepository;
use conta_verde_domain::{CompanyId, PartyId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::postgres::{connect, runtime_options};
use crate::tenant::{self, Unscoped};
use crate::{PostgresPartyRepository, contract};

/// The tables that have a `company_id` and are left out of the policies, each with the reason. A table that is not here
/// and has no policy fails `every_table_with_a_company_has_a_policy`.
const LEFT_OUT: [(&str, &str); 2] = [
    (
        "memberships",
        "read by user, across companies, to know which companies a person may open; it is what the scope is decided from",
    ),
    (
        "invitations",
        "found by the digest of its link, or by the invited address, before the person belongs to any company",
    ),
];

/// A pool as the server builds it: a request that was never narrowed to a company sees nothing.
async fn serving_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    connect(&url, Unscoped::Refuse).await.unwrap()
}

/// A party made as the schema's owner, who is not held to the policies, so the test does not depend on what it tests.
async fn a_party_of(owner: &PgPool, company: CompanyId) -> PartyId {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO parties (id, company_id, name, is_customer, is_supplier, is_carrier, is_seller, \
         document_check) VALUES ($1, $2, 'Padaria', true, false, false, false, 'not_applicable')",
    )
    .bind(id)
    .bind(company.as_uuid())
    .execute(owner)
    .await
    .unwrap();
    PartyId::from_uuid(id)
}

fn refused_by_a_policy<T>(result: Result<T, sqlx::Error>) -> bool {
    matches!(result, Err(sqlx::Error::Database(db)) if db.code().as_deref() == Some("42501"))
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn a_query_for_another_companys_rows_finds_nothing_and_a_write_is_refused() {
    let owner = contract::owner_pool().await;
    let mine = contract::create_company(&owner).await;
    let theirs = contract::create_company(&owner).await;
    let their_party = a_party_of(&owner, theirs).await;
    let parties = PostgresPartyRepository::new(serving_pool().await);

    // The company itself finds it.
    let found = tenant::within(theirs, parties.find(theirs, their_party)).await;
    assert!(found.unwrap().is_some());

    // The application's check is *skipped*: the repository is asked for the other company's id, which its own
    // `WHERE company_id = ...` accepts. Only the database stands between this request and that row.
    let skipped = tenant::within(mine, parties.find(theirs, their_party)).await;
    assert!(skipped.unwrap().is_none());

    let pool = serving_pool().await;
    let in_my_scope = |sql: &'static str| {
        let pool = pool.clone();
        async move {
            tenant::within(mine, async move {
                sqlx::query(sql).bind(theirs.as_uuid()).execute(&pool).await
            })
            .await
        }
    };
    // Reading, updating and deleting what is not in the scope touches no row, even when asked for by name.
    let counted = tenant::within(mine, async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM parties WHERE company_id = $1")
            .bind(theirs.as_uuid())
            .fetch_one(&pool)
            .await
    })
    .await;
    assert_eq!(counted.unwrap(), 0);
    let updated = in_my_scope("UPDATE parties SET name = 'Roubada' WHERE company_id = $1").await;
    assert_eq!(updated.unwrap().rows_affected(), 0);
    let deleted = in_my_scope("DELETE FROM parties WHERE company_id = $1").await;
    assert_eq!(deleted.unwrap().rows_affected(), 0);

    // Writing a row into the other company is refused outright, not silently dropped.
    let planted = tenant::within(mine, async {
        sqlx::query(
            "INSERT INTO parties (id, company_id, name, is_customer, is_supplier, is_carrier, is_seller, \
             document_check) VALUES (gen_random_uuid(), $1, 'Intrusa', true, false, false, false, 'not_applicable')",
        )
        .bind(theirs.as_uuid())
        .execute(&pool)
        .await
    })
    .await;
    assert!(refused_by_a_policy(planted));
    // ... and so is moving one of my own rows into theirs.
    let mine_party = tenant::within(mine, async {
        sqlx::query(
            "INSERT INTO parties (id, company_id, name, is_customer, is_supplier, is_carrier, is_seller, \
             document_check) VALUES (gen_random_uuid(), $1, 'Minha', true, false, false, false, 'not_applicable')",
        )
        .bind(mine.as_uuid())
        .execute(&pool)
        .await
    })
    .await;
    assert_eq!(mine_party.unwrap().rows_affected(), 1);
    let moved = tenant::within(mine, async {
        sqlx::query("UPDATE parties SET company_id = $1 WHERE company_id = $2")
            .bind(theirs.as_uuid())
            .bind(mine.as_uuid())
            .execute(&pool)
            .await
    })
    .await;
    assert!(refused_by_a_policy(moved));

    // What the other company had is untouched.
    let intact = tenant::within(theirs, parties.find(theirs, their_party)).await;
    assert_eq!(intact.unwrap().unwrap().name(), "Padaria");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn an_upsert_or_a_returning_clause_cannot_reach_another_companys_row() {
    let owner = contract::owner_pool().await;
    let mine = contract::create_company(&owner).await;
    let theirs = contract::create_company(&owner).await;
    let their_party = a_party_of(&owner, theirs).await;
    let pool = serving_pool().await;
    // The row exists, so `ON CONFLICT DO UPDATE` would update it were it visible; the policy refuses instead.
    let upsert = tenant::within(mine, async {
        sqlx::query(
            "INSERT INTO parties (id, company_id, name, is_customer, is_supplier, is_carrier, is_seller, \
             document_check) VALUES ($1, $2, 'Roubada', true, false, false, false, 'not_applicable') \
             ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name",
        )
        .bind(their_party.as_uuid())
        .bind(mine.as_uuid())
        .execute(&pool)
        .await
    })
    .await;
    assert!(upsert.is_err());
    let returned = tenant::within(mine, async {
        sqlx::query("UPDATE parties SET name = 'Roubada' WHERE id = $1 RETURNING id")
            .bind(their_party.as_uuid())
            .fetch_all(&pool)
            .await
    })
    .await;
    assert!(returned.unwrap().is_empty());
    let name: String = sqlx::query_scalar("SELECT name FROM parties WHERE id = $1")
        .bind(their_party.as_uuid())
        .fetch_one(&owner)
        .await
        .unwrap();
    assert_eq!(name, "Padaria");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn work_that_was_never_narrowed_sees_nothing_and_system_work_sees_every_company() {
    let owner = contract::owner_pool().await;
    let company = contract::create_company(&owner).await;
    let party = a_party_of(&owner, company).await;
    let parties = PostgresPartyRepository::new(serving_pool().await);

    // A handler that forgot to check who is asking: no scope was set, so nothing is found.
    assert!(parties.find(company, party).await.unwrap().is_none());
    let forgotten = tenant::scoped(parties.find(company, party)).await;
    assert!(forgotten.unwrap().is_none());
    // The workers and the hourly passes are system work and see everything.
    assert!(
        tenant::system(parties.find(company, party))
            .await
            .unwrap()
            .is_some()
    );
    // A request narrowed to the company, once its caller is known, finds it.
    let narrowed = tenant::scoped(async {
        tenant::set_company(company);
        parties.find(company, party).await
    })
    .await;
    assert!(narrowed.unwrap().is_some());
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn a_connection_never_keeps_the_scope_of_whoever_used_it_before() {
    let owner = contract::owner_pool().await;
    let mine = contract::create_company(&owner).await;
    let theirs = contract::create_company(&owner).await;
    let my_party = a_party_of(&owner, mine).await;
    let their_party = a_party_of(&owner, theirs).await;
    // One connection for everything, so each use is the next user of the same session.
    let url = std::env::var("DATABASE_URL").unwrap();
    let pool = runtime_options(Unscoped::Refuse)
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let parties = PostgresPartyRepository::new(pool);
    for _ in 0..5 {
        assert!(
            tenant::within(mine, parties.find(mine, my_party))
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            tenant::within(theirs, parties.find(mine, my_party))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            tenant::within(theirs, parties.find(theirs, their_party))
                .await
                .unwrap()
                .is_some()
        );
        assert!(parties.find(theirs, their_party).await.unwrap().is_none());
    }
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn concurrent_requests_each_see_only_their_own_company() {
    let owner = contract::owner_pool().await;
    let url = std::env::var("DATABASE_URL").unwrap();
    // Fewer connections than tasks, so connections are handed from one company to the next while others wait.
    let pool = runtime_options(Unscoped::Refuse)
        .max_connections(3)
        .connect(&url)
        .await
        .unwrap();
    let mut tasks = Vec::new();
    for _ in 0..12 {
        let company = contract::create_company(&owner).await;
        let party = a_party_of(&owner, company).await;
        let parties = PostgresPartyRepository::new(pool.clone());
        tasks.push(tokio::spawn(async move {
            let others = CompanyId::generate();
            for _ in 0..20 {
                let own = tenant::within(company, parties.find(company, party)).await;
                assert!(own.unwrap().is_some());
                let not_mine = tenant::within(others, parties.find(company, party)).await;
                assert!(not_mine.unwrap().is_none());
            }
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn every_table_with_a_company_has_a_policy() {
    let owner = contract::owner_pool().await;
    let unprotected: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname::text FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_attribute a ON a.attrelid = c.oid AND a.attname = 'company_id' AND NOT a.attisdropped \
         WHERE n.nspname = 'public' AND c.relkind = 'r' \
           AND NOT (c.relrowsecurity AND EXISTS (SELECT 1 FROM pg_policy p WHERE p.polrelid = c.oid)) \
         ORDER BY 1",
    )
    .fetch_all(&owner)
    .await
    .unwrap();
    let mut left_out: Vec<String> = LEFT_OUT
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect();
    left_out.sort();
    assert_eq!(
        unprotected, left_out,
        "a table with a company_id has no row-level policy: call cv_enable_tenancy('table') in its migration \
         (docs/adr/0093-row-level-security.md), or list it in LEFT_OUT with the reason it is read before a company is known"
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL"]
async fn the_server_does_not_serve_requests_as_a_role_the_policies_do_not_bind() {
    // The role that owns the schema is not held to the policies, so the server refuses to serve as it.
    let owner_url =
        std::env::var("DATABASE_MIGRATE_URL").expect("DATABASE_MIGRATE_URL must be set");
    let refusal = connect(&owner_url, Unscoped::Refuse).await.unwrap_err();
    assert!(
        refusal.to_string().contains("row-level security"),
        "{refusal}"
    );
}
