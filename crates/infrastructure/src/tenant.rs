//! Which company the work in hand acts for, so that the database can refuse the rest of them (ADR 0093).
//!
//! The scope lives in a task-local cell: a request gets an empty one at the edge (`scoped`), the extractor that
//! checked the caller's membership (or the gate that checked an API key) fills it with that company
//! (`set_company`), and every connection the pool then hands out to the same task is given the company as the
//! session setting `app.company_ids`, which the row-level policies read. Nothing is set until the caller is
//! known, so a handler that forgot to check sees no rows and cannot write any (the policies fail closed).
//!
//! Work that belongs to no company (the workers, the hourly passes) runs in the **system scope** (`system`): every
//! company is visible, and the application's own checks are the only barrier, as they were. A token that names a
//! company (a quote's link, an API key) is looked up in the system scope and then narrows the request to the
//! company it found.

use std::future::Future;
use std::sync::{Arc, Mutex};

use conta_verde_domain::CompanyId;
use sqlx::PgConnection;
use uuid::Uuid;

/// What the current task may see.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Scope {
    /// Nothing: the caller is not known yet.
    #[default]
    Nobody,
    Company(Uuid),
    /// Every company that exists (background work, and a token's lookup).
    System,
}

/// The scope of one request or one piece of background work, shared by everything the task does.
#[derive(Clone, Default)]
struct Cell(Arc<Mutex<Scope>>);

impl Cell {
    fn of(scope: Scope) -> Self {
        Self(Arc::new(Mutex::new(scope)))
    }

    fn get(&self) -> Scope {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn set(&self, scope: Scope) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = scope;
    }
}

tokio::task_local! {
    static CURRENT: Cell;
}

/// What a pool does for work that was not scoped at all: nothing may be seen, except in the adapters' own tests,
/// which exercise the stores without a request around them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unscoped {
    Refuse,
    #[cfg(test)]
    SeeEveryCompany,
}

/// Runs `work` with a scope of its own that starts empty: the edge of a request.
pub async fn scoped<F: Future>(work: F) -> F::Output {
    CURRENT.scope(Cell::default(), work).await
}

/// Runs `work` as the work of `company` only, whatever the surrounding scope was.
pub async fn within<F: Future>(company: CompanyId, work: F) -> F::Output {
    CURRENT
        .scope(Cell::of(Scope::Company(company.as_uuid())), work)
        .await
}

/// Runs `work` as system work: every company is visible (the workers and the hourly passes, a token's lookup).
pub async fn system<F: Future>(work: F) -> F::Output {
    CURRENT.scope(Cell::of(Scope::System), work).await
}

/// Narrows what the current request does to `company`, once the caller's right to it was checked. Nothing happens
/// outside a scope (the in-memory adapters' tests have none).
pub fn set_company(company: CompanyId) {
    let _ = CURRENT.try_with(|cell| cell.set(Scope::Company(company.as_uuid())));
}

fn current() -> Option<Scope> {
    CURRENT.try_with(Cell::get).ok()
}

/// Hands the scope of the task that is acquiring a connection to the connection, as `app.company_ids`: a comma
/// separated list that the policies compare the row's company with. It is set on every acquire, so a connection
/// never keeps the scope of whoever used it before.
pub(crate) async fn apply(conn: &mut PgConnection, unscoped: Unscoped) -> Result<(), sqlx::Error> {
    let scope = current().unwrap_or(match unscoped {
        Unscoped::Refuse => Scope::Nobody,
        #[cfg(test)]
        Unscoped::SeeEveryCompany => Scope::System,
    });
    match scope {
        Scope::Nobody => set(conn, "").await,
        Scope::Company(company) => set(conn, &company.to_string()).await,
        Scope::System => sqlx::query(
            "SELECT set_config('app.company_ids', \
             COALESCE((SELECT string_agg(id::text, ',') FROM companies), ''), false)",
        )
        .execute(conn)
        .await
        .map(|_| ()),
    }
}

async fn set(conn: &mut PgConnection, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config('app.company_ids', $1, false)")
        .bind(value)
        .execute(conn)
        .await
        .map(|_| ())
}
