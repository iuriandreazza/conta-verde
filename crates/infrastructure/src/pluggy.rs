use std::str::FromStr;
use std::time::{Duration, Instant};

use conta_verde_application::{
    AggregatedAccount, AggregatedTransaction, AggregatorError, BankAggregator, ItemState,
};
use conta_verde_domain::{CalendarDate, CompanyId, Decimal};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::brasil_api::client_without_redirects;

const DEFAULT_API_URL: &str = "https://api.pluggy.ai";
/// The API key is asked for again after this long and whenever it is refused. The public documentation does not say how long a key
/// lives, so this is a guess that errs on the short side.
const KEY_LIFETIME: Duration = Duration::from_secs(45 * 60);
/// A pull stops after this many pages (500 movements each): more than a company's account makes in the days a pull covers.
const MAX_PAGES: usize = 100;

/// The Pluggy aggregator ([ADR 0089]), written from Pluggy's **public documentation** (docs.pluggy.ai: `POST /auth`,
/// `POST /connect_token`, `GET /accounts`, `GET /items/{id}`, `GET /v2/transactions` with its cursor, `DELETE /items/{id}`) and
/// **never exercised against the real service**: it has only been run against a local stand-in written from the same pages. What
/// the pages leave open, and this adapter therefore assumes and a sandbox account must confirm: how long an API key lives; the sign
/// of the amount of a bank account's movement (it uses `type`, `CREDIT` in and `DEBIT` out, and ignores the sign it came with);
/// the time zone of a movement's date (it takes the date part); the values of an item's `status` (it treats `LOGIN_ERROR`,
/// `WAITING_USER_INPUT` and `WAITING_USER_ACTION` as needing the person); and what `next` of the cursor holds (a query string,
/// with or without a leading `?`).
///
/// **The credentials are the installation's** (`PLUGGY_CLIENT_ID`, `PLUGGY_CLIENT_SECRET`): one Pluggy account per installation,
/// never stored per company, never logged. Nothing an answer says is logged but its status.
pub struct PluggyAggregator {
    client: reqwest::Client,
    base_url: String,
    client_id: String,
    client_secret: String,
    key: tokio::sync::Mutex<Option<(String, Instant)>>,
}

impl PluggyAggregator {
    pub fn new(
        client_id: &str,
        client_secret: &str,
        base_url: Option<&str>,
    ) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: client_without_redirects(Duration::from_secs(30))?,
            base_url: base_url
                .unwrap_or(DEFAULT_API_URL)
                .trim_end_matches('/')
                .to_owned(),
            client_id: client_id.to_owned(),
            client_secret: client_secret.to_owned(),
            key: tokio::sync::Mutex::new(None),
        })
    }

    /// The API key, asked for when there is none, when it is old, or when `fresh` says the one held was refused.
    async fn api_key(&self, fresh: bool) -> Result<String, AggregatorError> {
        let mut held = self.key.lock().await;
        if !fresh
            && let Some((key, at)) = held.as_ref()
            && at.elapsed() < KEY_LIFETIME
        {
            return Ok(key.clone());
        }
        let response = self
            .client
            .post(format!("{}/auth", self.base_url))
            .json(&json!({ "clientId": self.client_id, "clientSecret": self.client_secret }))
            .send()
            .await
            .map_err(unreachable)?;
        let status = response.status();
        if !status.is_success() {
            // The credentials are the operator's: a refusal is theirs to fix, and is said as such, without the answer.
            return Err(AggregatorError::Refused(format!(
                "Pluggy refused the credentials ({status})"
            )));
        }
        let answer: Value = response.json().await.unwrap_or(Value::Null);
        let key = answer["apiKey"]
            .as_str()
            .filter(|key| !key.is_empty())
            .ok_or_else(|| AggregatorError::Unavailable("Pluggy gave no API key".to_owned()))?
            .to_owned();
        *held = Some((key.clone(), Instant::now()));
        Ok(key)
    }

    /// One call with the key, asking for another once if the first is refused. The answer's JSON, or why not.
    async fn call(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
        body: Option<Value>,
    ) -> Result<Value, AggregatorError> {
        for attempt in 0..2 {
            let key = self.api_key(attempt > 0).await?;
            let mut request = self
                .client
                .request(
                    method.clone(),
                    format!("{}{path}{}", self.base_url, query_string(query)),
                )
                .header("X-API-KEY", key);
            if let Some(body) = &body {
                request = request.json(body);
            }
            let response = request.send().await.map_err(unreachable)?;
            let status = response.status();
            if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) && attempt == 0 {
                continue;
            }
            if status.is_success() {
                return Ok(response.json().await.unwrap_or(Value::Null));
            }
            return Err(match status {
                StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_MANY_REQUESTS => {
                    AggregatorError::Unavailable(format!("Pluggy answered {status}"))
                }
                status if status.is_server_error() => {
                    AggregatorError::Unavailable(format!("Pluggy answered {status}"))
                }
                status => AggregatorError::Refused(format!("Pluggy answered {status}")),
            });
        }
        Err(AggregatorError::Refused(
            "Pluggy refused the API key".to_owned(),
        ))
    }
}

/// A request that did not get an answer, without the address (it can hold an id of the person's bank connection).
fn unreachable(error: reqwest::Error) -> AggregatorError {
    AggregatorError::Unavailable(format!("request failed: {}", error.without_url()))
}

impl BankAggregator for PluggyAggregator {
    fn provider(&self) -> &'static str {
        "pluggy"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn connect_token(
        &self,
        company_id: CompanyId,
        item: Option<&str>,
    ) -> Result<String, AggregatorError> {
        let mut body = json!({ "options": { "clientUserId": company_id.as_uuid().to_string() } });
        if let Some(item) = item {
            body["itemId"] = json!(item);
        }
        let answer = self
            .call(Method::POST, "/connect_token", &[], Some(body))
            .await?;
        answer["accessToken"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| AggregatorError::Unavailable("Pluggy gave no connect token".to_owned()))
    }

    async fn accounts(&self, item: &str) -> Result<Vec<AggregatedAccount>, AggregatorError> {
        let answer = self
            .call(
                Method::GET,
                "/accounts",
                &[("itemId".to_owned(), item.to_owned())],
                None,
            )
            .await?;
        Ok(answer["results"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|account| {
                let id = account["id"].as_str()?.to_owned();
                let name = account["marketingName"]
                    .as_str()
                    .or_else(|| account["name"].as_str())
                    .unwrap_or("Account")
                    .to_owned();
                Some(AggregatedAccount {
                    id,
                    name,
                    kind: account["subtype"].as_str().unwrap_or("").to_owned(),
                    number: account["number"].as_str().map(str::to_owned),
                })
            })
            .collect())
    }

    async fn item_state(&self, item: &str) -> Result<ItemState, AggregatorError> {
        let answer = self
            .call(Method::GET, &format!("/items/{item}"), &[], None)
            .await?;
        Ok(match answer["status"].as_str() {
            Some("LOGIN_ERROR") => ItemState::NeedsAttention("the bank login failed".to_owned()),
            Some("WAITING_USER_INPUT" | "WAITING_USER_ACTION") => {
                ItemState::NeedsAttention("the bank is waiting for the person".to_owned())
            }
            _ => ItemState::Ready,
        })
    }

    async fn transactions(
        &self,
        account: &str,
        from: CalendarDate,
        to: CalendarDate,
    ) -> Result<Vec<AggregatedTransaction>, AggregatorError> {
        let mut query = vec![
            ("accountId".to_owned(), account.to_owned()),
            ("dateFrom".to_owned(), from.to_string()),
            ("dateTo".to_owned(), to.to_string()),
        ];
        let mut found = Vec::new();
        for _ in 0..MAX_PAGES {
            let answer = self
                .call(Method::GET, "/v2/transactions", &query, None)
                .await?;
            found.extend(
                answer["results"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(transaction_of),
            );
            // `next` is a ready query string, or null on the last page; the account and the days stay in it.
            let Some(next) = answer["next"].as_str().filter(|next| !next.is_empty()) else {
                return Ok(found);
            };
            query = next
                .trim_start_matches('?')
                .split('&')
                .filter_map(|pair| pair.split_once('='))
                // Only the names the endpoint takes are forwarded: whatever else `next` holds is dropped.
                .filter(|(name, _)| {
                    matches!(
                        *name,
                        "accountId" | "dateFrom" | "dateTo" | "createdAtFrom" | "after"
                    )
                })
                .map(|(name, value)| (name.to_owned(), decode(value)))
                .collect();
        }
        Err(AggregatorError::Unavailable(
            "the pull had too many pages".to_owned(),
        ))
    }

    async fn disconnect(&self, item: &str) -> Result<(), AggregatorError> {
        self.call(Method::DELETE, &format!("/items/{item}"), &[], None)
            .await
            .map(|_| ())
    }
}

/// `?name=value&...` with every part percent-encoded, or nothing when there is nothing to ask.
fn query_string(query: &[(String, String)]) -> String {
    if query.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = query
        .iter()
        .map(|(name, value)| format!("{}={}", encode(name), encode(value)))
        .collect();
    format!("?{}", parts.join("&"))
}

fn encode(raw: &str) -> String {
    raw.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// A query value as it was sent: `%XX` and `+` undone.
fn decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 3 <= bytes.len()
                && raw.is_char_boundary(i + 1)
                && raw.is_char_boundary(i + 3) =>
            {
                match u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            byte => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A posted movement as a line the sync can store, or `None` for what is not one (pending, no id, no date, no amount).
fn transaction_of(raw: &Value) -> Option<AggregatedTransaction> {
    // A pending movement becomes posted later under the same id and is stored then.
    if !raw["status"]
        .as_str()
        .is_some_and(|status| status.eq_ignore_ascii_case("POSTED"))
    {
        return None;
    }
    let id = raw["id"].as_str()?.to_owned();
    let posted_on = CalendarDate::parse(raw["date"].as_str()?.get(..10)?)?;
    let magnitude = Decimal::from_str(&raw["amount"].as_number()?.to_string())
        .ok()?
        .abs();
    // A bank account's amount has no sign convention the documentation states: the direction is `type`.
    let amount = match raw["type"].as_str() {
        Some("CREDIT") => magnitude,
        Some("DEBIT") => -magnitude,
        _ => Decimal::from_str(&raw["amount"].as_number()?.to_string()).ok()?,
    };
    let description = raw["description"]
        .as_str()
        .or_else(|| raw["descriptionRaw"].as_str())?
        .to_owned();
    Some(AggregatedTransaction {
        id,
        posted_on,
        amount: amount.normalize().to_string(),
        description,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::extract::{Path, Query, State};
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::{delete, get, post};
    use axum::{Json, extract::Request};

    use super::*;

    /// A local stand-in for Pluggy written from its public documentation: the tests never reach the real one.
    #[derive(Default)]
    struct World {
        /// How many API keys were asked for, and which key is the good one now.
        keys_made: u32,
        refuse_first_key: bool,
        credentials_ok: bool,
        deleted: Vec<String>,
        item_status: String,
        pages_asked: Vec<HashMap<String, String>>,
        fail_with: Option<u16>,
    }

    type Shared = Arc<Mutex<World>>;

    const GOOD_ID: &str = "00000000-0000-0000-0000-00000000c0de";
    const GOOD_SECRET: &str = "the-client-secret";

    async fn auth(
        State(world): State<Shared>,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let mut world = world.lock().unwrap();
        if body["clientId"] != GOOD_ID
            || body["clientSecret"] != GOOD_SECRET
            || !world.credentials_ok
        {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "message": "secret reason that must not leak" })),
            );
        }
        world.keys_made += 1;
        (
            StatusCode::OK,
            Json(json!({ "apiKey": format!("key-{}", world.keys_made) })),
        )
    }

    /// Whether the key is the good one: the first is refused once when the test asks for it.
    fn key_ok(world: &World, headers: &HeaderMap) -> bool {
        let key = headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        !(world.refuse_first_key && key == "key-1")
    }

    async fn connect_token(
        State(world): State<Shared>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let world = world.lock().unwrap();
        if !key_ok(&world, &headers) {
            return (StatusCode::FORBIDDEN, Json(Value::Null));
        }
        (
            StatusCode::OK,
            Json(
                json!({ "accessToken": format!("connect-{}", body["options"]["clientUserId"].as_str().unwrap_or("")) }),
            ),
        )
    }

    async fn accounts(
        State(world): State<Shared>,
        headers: HeaderMap,
        Query(query): Query<HashMap<String, String>>,
    ) -> (StatusCode, Json<Value>) {
        let world = world.lock().unwrap();
        if !key_ok(&world, &headers) {
            return (StatusCode::FORBIDDEN, Json(Value::Null));
        }
        if let Some(code) = world.fail_with {
            return (
                StatusCode::from_u16(code).unwrap(),
                Json(json!({ "message": "do not leak" })),
            );
        }
        if query.get("itemId").map(String::as_str) != Some("item-1") {
            return (StatusCode::NOT_FOUND, Json(Value::Null));
        }
        (
            StatusCode::OK,
            Json(json!({ "page": 1, "total": 2, "totalPages": 1, "results": [
                { "id": "acc-1", "type": "BANK", "subtype": "CHECKING_ACCOUNT", "name": "Conta", "marketingName": "Conta Corrente", "number": "1234-5" },
                { "id": "acc-2", "type": "CREDIT", "subtype": "CREDIT_CARD", "name": "Cartão" }
            ] })),
        )
    }

    async fn item(State(world): State<Shared>, Path(id): Path<String>) -> Json<Value> {
        let world = world.lock().unwrap();
        Json(json!({ "id": id, "status": world.item_status, "executionStatus": "SUCCESS" }))
    }

    async fn delete_item(State(world): State<Shared>, Path(id): Path<String>) -> StatusCode {
        world.lock().unwrap().deleted.push(id);
        StatusCode::NO_CONTENT
    }

    fn tx(
        id: &str,
        date: &str,
        amount: Value,
        kind: &str,
        status: &str,
        description: &str,
    ) -> Value {
        json!({ "id": id, "accountId": "acc-1", "date": date, "amount": amount, "type": kind, "status": status,
                "description": description, "currencyCode": "BRL" })
    }

    async fn transactions(
        State(world): State<Shared>,
        Query(query): Query<HashMap<String, String>>,
    ) -> Json<Value> {
        world.lock().unwrap().pages_asked.push(query.clone());
        // Two pages: the first says what comes next in a ready query string, the second says nothing does.
        if query.get("after").map(String::as_str) == Some("cursor 2/=") {
            return Json(json!({ "results": [
                tx("t3", "2026-11-03T00:00:00.000Z", json!(1000), "CREDIT", "POSTED", "SALARIO"),
            ], "next": null }));
        }
        Json(json!({ "results": [
            tx("t1", "2026-11-01T00:00:00.000Z", json!(-212.45), "DEBIT", "POSTED", "MERCADO"),
            // A debit that comes with a positive sign, and a credit with a negative one: the type decides.
            tx("t2", "2026-11-02T12:30:00.000Z", json!(30), "DEBIT", "POSTED", "TARIFA"),
            tx("t4", "2026-11-02T00:00:00.000Z", json!(-5), "CREDIT", "POSTED", "ESTORNO"),
            tx("t5", "2026-11-02T00:00:00.000Z", json!(-99), "DEBIT", "PENDING", "AINDA NAO"),
            json!({ "id": "t6", "date": "2026-11-02", "amount": 3, "type": "CREDIT", "status": "POSTED" }),
        ], "next": "accountId=acc-1&dateFrom=2026-11-01&dateTo=2026-11-10&after=cursor%202%2F%3D&evil=1" }))
    }

    async fn start(world: Shared) -> String {
        let app = Router::new()
            .route("/auth", post(auth))
            .route("/connect_token", post(connect_token))
            .route("/accounts", get(accounts))
            .route("/items/{id}", get(item).delete(delete_item))
            .route("/items/{id}/x", delete(delete_item))
            .route("/v2/transactions", get(transactions))
            .fallback(|_: Request| async { StatusCode::NOT_FOUND })
            .with_state(world);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        url
    }

    fn world() -> Shared {
        Arc::new(Mutex::new(World {
            credentials_ok: true,
            item_status: "UPDATED".to_owned(),
            ..World::default()
        }))
    }

    async fn pluggy(world: &Shared) -> PluggyAggregator {
        let url = start(world.clone()).await;
        PluggyAggregator::new(GOOD_ID, GOOD_SECRET, Some(&url)).unwrap()
    }

    fn day(text: &str) -> CalendarDate {
        CalendarDate::parse(text).unwrap()
    }

    #[tokio::test]
    async fn it_lists_the_accounts_of_a_connection_and_asks_for_the_key_once() {
        let world = world();
        let aggregator = pluggy(&world).await;
        let accounts = aggregator.accounts("item-1").await.unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(
            (
                accounts[0].id.as_str(),
                accounts[0].name.as_str(),
                accounts[0].kind.as_str(),
                accounts[0].number.as_deref()
            ),
            (
                "acc-1",
                "Conta Corrente",
                "CHECKING_ACCOUNT",
                Some("1234-5")
            )
        );
        aggregator.accounts("item-1").await.unwrap();
        assert_eq!(world.lock().unwrap().keys_made, 1);
        // An item that is not Pluggy's is refused, with a reason that is ours and not what Pluggy said.
        match aggregator.accounts("nope").await.unwrap_err() {
            AggregatorError::Refused(reason) => assert!(!reason.contains("leak")),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn a_key_that_is_refused_is_asked_for_again_once() {
        let world = world();
        world.lock().unwrap().refuse_first_key = true;
        let aggregator = pluggy(&world).await;
        assert_eq!(aggregator.accounts("item-1").await.unwrap().len(), 2);
        assert_eq!(world.lock().unwrap().keys_made, 2);
    }

    #[tokio::test]
    async fn bad_credentials_and_a_server_that_fails_are_told_apart_and_never_quote_the_answer() {
        let world = world();
        world.lock().unwrap().credentials_ok = false;
        let aggregator = pluggy(&world).await;
        match aggregator.accounts("item-1").await.unwrap_err() {
            AggregatorError::Refused(reason) => {
                assert!(reason.contains("credentials") && !reason.contains("secret reason"));
                assert!(!reason.contains(GOOD_SECRET));
            }
            other => panic!("{other:?}"),
        }
        world.lock().unwrap().credentials_ok = true;
        for (code, retry) in [(500, true), (503, true), (429, true), (400, false)] {
            world.lock().unwrap().fail_with = Some(code);
            let error = aggregator.accounts("item-1").await.unwrap_err();
            assert_eq!(
                matches!(error, AggregatorError::Unavailable(_)),
                retry,
                "{code}: {error:?}"
            );
            assert!(!error.to_string().contains("do not leak"));
        }
        // Nothing listening: unavailable, and the address is not in the reason.
        let down = PluggyAggregator::new(GOOD_ID, GOOD_SECRET, Some("http://127.0.0.1:1")).unwrap();
        let error = down.accounts("item-1").await.unwrap_err();
        assert!(matches!(error, AggregatorError::Unavailable(_)));
        assert!(!error.to_string().contains("127.0.0.1"));
    }

    #[tokio::test]
    async fn the_transactions_of_every_page_are_posted_ones_signed_by_their_type() {
        let world = world();
        let aggregator = pluggy(&world).await;
        let found = aggregator
            .transactions("acc-1", day("2026-11-01"), day("2026-11-10"))
            .await
            .unwrap();
        let by_id: HashMap<&str, &AggregatedTransaction> =
            found.iter().map(|t| (t.id.as_str(), t)).collect();
        // The pending one and the one with no description are left out; the three pages' worth are all there.
        assert_eq!(by_id.len(), 4, "{found:?}");
        assert_eq!(
            (by_id["t1"].amount.as_str(), by_id["t1"].posted_on),
            ("-212.45", day("2026-11-01"))
        );
        assert_eq!(by_id["t2"].amount, "-30");
        assert_eq!(by_id["t4"].amount, "5");
        assert_eq!(
            (
                by_id["t3"].amount.as_str(),
                by_id["t3"].description.as_str()
            ),
            ("1000", "SALARIO")
        );
        // The first page asked for the account and the days; the second only what `next` said, decoded, and nothing else.
        let asked = world.lock().unwrap().pages_asked.clone();
        assert_eq!(asked.len(), 2);
        assert_eq!(asked[0]["accountId"], "acc-1");
        assert_eq!(
            (asked[0]["dateFrom"].as_str(), asked[0]["dateTo"].as_str()),
            ("2026-11-01", "2026-11-10")
        );
        assert_eq!(asked[1]["after"], "cursor 2/=");
        assert!(!asked[1].contains_key("evil"));
    }

    #[tokio::test]
    async fn an_item_that_needs_the_person_is_told_and_a_connection_is_ended() {
        let world = world();
        let aggregator = pluggy(&world).await;
        assert_eq!(
            aggregator.item_state("item-1").await.unwrap(),
            ItemState::Ready
        );
        for status in ["LOGIN_ERROR", "WAITING_USER_INPUT", "WAITING_USER_ACTION"] {
            world.lock().unwrap().item_status = status.to_owned();
            assert!(
                matches!(
                    aggregator.item_state("item-1").await.unwrap(),
                    ItemState::NeedsAttention(_)
                ),
                "{status}"
            );
        }
        world.lock().unwrap().item_status = "OUTDATED".to_owned();
        assert_eq!(
            aggregator.item_state("item-1").await.unwrap(),
            ItemState::Ready
        );
        aggregator.disconnect("item-1").await.unwrap();
        assert_eq!(world.lock().unwrap().deleted, ["item-1"]);
    }

    #[tokio::test]
    async fn a_connect_token_is_asked_for_the_company_and_the_secrets_are_never_in_a_debug() {
        let world = world();
        let aggregator = pluggy(&world).await;
        let company = CompanyId::generate();
        let token = aggregator.connect_token(company, None).await.unwrap();
        assert_eq!(token, format!("connect-{}", company.as_uuid()));
        assert!(
            aggregator
                .connect_token(company, Some("item-1"))
                .await
                .is_ok()
        );
    }

    #[test]
    fn the_query_is_percent_encoded_and_decoded_whole() {
        let query = vec![("after".to_owned(), "a b/c=&é".to_owned())];
        let text = query_string(&query);
        assert_eq!(text, "?after=a%20b%2Fc%3D%26%C3%A9");
        assert_eq!(decode("a%20b%2Fc%3D%26%C3%A9"), "a b/c=&é");
        assert_eq!(decode("100%"), "100%");
        assert_eq!(decode("%zz+x"), "%zz x");
        assert_eq!(query_string(&[]), "");
    }
}
