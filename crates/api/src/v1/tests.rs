// Included in the `tests` module of `lib.rs`, so it uses the helpers of the other API tests ([ADR 0087]).

const V1_OPENAPI: &str = include_str!("../../../../docs/api/v1/openapi.json");
const V1_FROZEN: &str = include_str!("../../../../docs/api/v1/openapi.frozen.json");
const V1_ROUTER_SOURCE: &str = include_str!("mod.rs");

fn bearer_request(method: &str, uri: &str, secret: &str, body: Option<Value>) -> Request<Body> {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {secret}"))
        .header(header::CONTENT_TYPE, "application/json");
    builder
        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
        .unwrap()
}

/// What an answer of `v1` says: its status, its headers and its JSON.
struct V1Reply {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Value,
}

async fn v1(app: &Router, method: &str, uri: &str, secret: &str, body: Option<Value>) -> V1Reply {
    let (status, headers, bytes) = raw(app, bearer_request(method, uri, secret, body)).await;
    V1Reply {
        status,
        headers,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

/// Makes a key through the web app's API and returns its secret and id.
async fn make_key(
    app: &Router,
    cookie: &str,
    company_id: Uuid,
    scopes: &[&str],
) -> (String, String) {
    let reply = send(
        app,
        json_request(
            "POST",
            &format!("/companies/{company_id}/api-keys"),
            Some(cookie),
            json!({ "name": "Loja online", "scopes": scopes }),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    (
        reply.body["secret"].as_str().unwrap().to_owned(),
        reply.body["key"]["id"].as_str().unwrap().to_owned(),
    )
}

/// The schema `name` of the checked-in document.
fn v1_schema(name: &str) -> Value {
    let document: Value = serde_json::from_str(V1_OPENAPI).unwrap();
    document["components"]["schemas"][name].clone()
}

/// Checks `value` against a schema of the document with what the document uses of JSON Schema: `$ref`, `type` (also a list),
/// `properties` that are all there are, `required`, `items`, `enum`, `oneOf` and the formats `uuid` and `date`. A shape the
/// handler sends and the document does not say (or the other way round) fails here.
fn v1_conforms(schema: &Value, value: &Value, at: &str) -> Result<(), String> {
    let document: Value = serde_json::from_str(V1_OPENAPI).unwrap();
    conforms(&document, schema, value, at)
}

fn conforms(document: &Value, schema: &Value, value: &Value, at: &str) -> Result<(), String> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let name = reference.rsplit('/').next().unwrap();
        return conforms(document, &document["components"]["schemas"][name], value, at);
    }
    if let Some(options) = schema.get("oneOf").and_then(Value::as_array) {
        let passing = options
            .iter()
            .filter(|option| conforms(document, option, value, at).is_ok())
            .count();
        return if passing == 1 {
            Ok(())
        } else {
            Err(format!("{at}: {passing} of the oneOf options fit {value}"))
        };
    }
    let kinds: Vec<&str> = match &schema["type"] {
        Value::String(kind) => vec![kind.as_str()],
        Value::Array(kinds) => kinds.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let kind_of = |value: &Value| match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    };
    if !kinds.is_empty() && !kinds.contains(&kind_of(value)) {
        return Err(format!("{at}: {} is not of type {kinds:?}", kind_of(value)));
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        return Err(format!("{at}: {value} is not one of {allowed:?}"));
    }
    match (schema.get("format").and_then(Value::as_str), value.as_str()) {
        (Some("uuid"), Some(text)) if Uuid::parse_str(text).is_err() => {
            return Err(format!("{at}: {text} is not a uuid"));
        }
        (Some("date"), Some(text)) if conta_verde_domain::CalendarDate::parse(text).is_none() => {
            return Err(format!("{at}: {text} is not a date"));
        }
        _ => {}
    }
    if let Some(object) = value.as_object() {
        let properties = schema["properties"].as_object();
        for key in object.keys() {
            if !properties.is_some_and(|properties| properties.contains_key(key)) {
                return Err(format!("{at}: `{key}` is not in the document"));
            }
        }
        for key in schema["required"].as_array().into_iter().flatten() {
            let key = key.as_str().unwrap();
            if !object.contains_key(key) {
                return Err(format!("{at}: `{key}` is required and missing"));
            }
        }
        for (key, inner) in object {
            conforms(
                document,
                &properties.unwrap()[key],
                inner,
                &format!("{at}.{key}"),
            )?;
        }
    }
    if let (Some(items), Some(values)) = (schema.get("items"), value.as_array()) {
        for (index, inner) in values.iter().enumerate() {
            conforms(document, items, inner, &format!("{at}[{index}]"))?;
        }
    }
    Ok(())
}

fn assert_v1_shape(name: &str, value: &Value) {
    if let Err(problem) = v1_conforms(&v1_schema(name), value, name) {
        panic!("{problem}");
    }
}

/// What `frozen` says, `current` still says: every path, method, parameter and response, every property of every schema
/// with the same type, every enum value, and no more required in a request. Descriptions may change.
fn v1_only_grows(frozen: &Value, current: &Value, at: &str, problems: &mut Vec<String>) {
    match (frozen, current) {
        (Value::Object(old), Value::Object(new)) => {
            for (key, inner) in old {
                // Prose may change, unless it is the name of a property (a field called `description` is a field).
                if !at.ends_with(".properties")
                    && matches!(key.as_str(), "description" | "summary" | "title" | "version")
                {
                    continue;
                }
                match new.get(key) {
                    None => problems.push(format!("{at}.{key} was removed")),
                    Some(now) => v1_only_grows(inner, now, &format!("{at}.{key}"), problems),
                }
            }
        }
        (Value::Array(old), Value::Array(new)) => {
            let is_required = at.ends_with(".required");
            let is_request = at.contains("Request.") || at.contains("requestBody");
            if is_required {
                // A request may not ask for more than it did; a response may promise more.
                let (smaller, larger) = if is_request { (new, old) } else { (old, new) };
                for key in smaller {
                    if !larger.contains(key) {
                        problems.push(format!("{at}: {key} changed whether it is required"));
                    }
                }
            } else if old.iter().all(Value::is_string) {
                // An enum (or a list of names) may gain values, never lose one.
                for value in old {
                    if !new.contains(value) {
                        problems.push(format!("{at}: {value} was removed"));
                    }
                }
            } else if old.iter().all(|item| item.get("name").is_some()) {
                for item in old {
                    let name = &item["name"];
                    match new.iter().find(|candidate| &candidate["name"] == name) {
                        None => problems.push(format!("{at}: parameter {name} was removed")),
                        Some(now) => v1_only_grows(item, now, &format!("{at}.{name}"), problems),
                    }
                }
            } else if old != new {
                problems.push(format!("{at} changed"));
            }
        }
        (old, new) => {
            if old != new {
                problems.push(format!("{at}: {old} became {new}"));
            }
        }
    }
}

/// The routes of `v1`, with their methods, read from the source of `routes` so that a route added without the document is
/// found by the tests below.
fn v1_routes() -> Vec<(String, Vec<&'static str>)> {
    let source = V1_ROUTER_SOURCE;
    let source = &source[source.find("pub(crate) fn routes").unwrap()..];
    let source = &source[..source.find("async fn openapi").unwrap()];
    let mut routes = Vec::new();
    for (at, _) in source.match_indices(".route(") {
        let body = &source[at + ".route(".len()..];
        let mut depth = 1;
        let mut end = body.len();
        for (index, character) in body.char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = index;
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &body[..end];
        let opening = body.find('"').unwrap();
        let closing = opening + 1 + body[opening + 1..].find('"').unwrap();
        let path = body[opening + 1..closing].to_owned();
        let chain = &body[closing + 1..];
        let methods: Vec<&'static str> = [("get", "get"), ("post", "post"), ("put", "put"), ("delete", "delete")]
            .into_iter()
            .filter(|(name, _)| {
                chain.match_indices(&format!("{name}(")).any(|(index, _)| {
                    !chain[..index]
                        .chars()
                        .next_back()
                        .is_some_and(|before| before.is_alphanumeric() || before == '_')
                })
            })
            .map(|(_, method)| method)
            .collect();
        assert!(!methods.is_empty(), "no method found for {path}");
        routes.push((path, methods));
    }
    routes
}

#[test]
fn the_document_and_the_router_have_the_same_routes_and_every_route_has_an_area() {
    let document: Value = serde_json::from_str(V1_OPENAPI).unwrap();
    let routes = v1_routes();
    assert!(routes.len() >= 12, "{} routes", routes.len());
    let mut in_router: Vec<String> = routes
        .iter()
        .flat_map(|(path, methods)| methods.iter().map(move |method| format!("{method} {path}")))
        .collect();
    let mut in_document: Vec<String> = document["paths"]
        .as_object()
        .unwrap()
        .iter()
        .flat_map(|(path, item)| {
            item.as_object()
                .unwrap()
                .keys()
                .map(move |method| format!("{method} {path}"))
        })
        .collect();
    in_router.sort();
    in_document.sort();
    assert_eq!(in_router, in_document);
    // Every route that needs a key has an area, and no area a key may not have.
    for (path, _) in &routes {
        if path == "/v1/openapi.json" {
            continue;
        }
        let area = v1::area_of(path).unwrap_or_else(|| panic!("{path} has no area"));
        assert!(conta_verde_domain::API_KEY_AREAS.contains(&area), "{path}");
    }
    assert_eq!(v1::ROUTE_AREAS.len(), routes.len() - 1);
    // Every parameter of a path is declared, and every operation says what scope it needs.
    for (path, item) in document["paths"].as_object().unwrap() {
        for (method, operation) in item.as_object().unwrap() {
            for parameter in path.split('{').skip(1) {
                let name = parameter.split('}').next().unwrap();
                assert!(
                    operation["parameters"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|declared| declared["name"] == name && declared["in"] == "path"),
                    "{method} {path} does not declare {name}"
                );
            }
        }
    }
}

#[tokio::test]
async fn every_route_of_v1_but_the_document_refuses_a_request_without_a_key() {
    let (app, _) = entry_app_with_identities();
    for (path, methods) in v1_routes() {
        if path == "/v1/openapi.json" {
            continue;
        }
        let uri = path
            .replace("{party_id}", "00000000-0000-0000-0000-000000000000")
            .replace("{product_id}", "00000000-0000-0000-0000-000000000000")
            .replace("{sale_id}", "00000000-0000-0000-0000-000000000000");
        for method in methods {
            let request = Request::builder()
                .method(method.to_uppercase().as_str())
                .uri(&uri)
                .body(Body::empty())
                .unwrap();
            let (status, headers, _) = raw(&app, request).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
            assert_eq!(headers[header::CONTENT_TYPE], "application/problem+json", "{method} {path}");
        }
    }
}

#[test]
fn the_document_only_grows() {
    let frozen: Value = serde_json::from_str(V1_FROZEN).unwrap();
    let current: Value = serde_json::from_str(V1_OPENAPI).unwrap();
    let mut problems = Vec::new();
    v1_only_grows(&frozen, &current, "", &mut problems);
    assert!(
        problems.is_empty(),
        "v1 lost something it promised (a break goes to v2):\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_checker_of_shapes_refuses_what_the_document_does_not_say() {
    let sale = json!({ "items": [{ "id": "nope" }], "next_cursor": null });
    assert!(v1_conforms(&v1_schema("SalePage"), &sale, "SalePage").is_err());
    let extra = json!({ "items": [], "next_cursor": null, "surprise": 1 });
    assert!(v1_conforms(&v1_schema("SalePage"), &extra, "SalePage").is_err());
    let fine = json!({ "items": [], "next_cursor": null });
    assert!(v1_conforms(&v1_schema("SalePage"), &fine, "SalePage").is_ok());
    // The tripwire notices a removed property and a request that asks for more.
    let frozen: Value = serde_json::from_str(V1_FROZEN).unwrap();
    let mut changed = frozen.clone();
    changed["components"]["schemas"]["Party"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("tags");
    changed["components"]["schemas"]["SaleRequest"]["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("notes"));
    let mut problems = Vec::new();
    v1_only_grows(&frozen, &changed, "", &mut problems);
    assert!(problems.iter().any(|problem| problem.contains("tags was removed")));
    // A property that is called `description` is a property, not prose.
    let mut renamed = frozen.clone();
    renamed["components"]["schemas"]["Line"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("description");
    let mut lost = Vec::new();
    v1_only_grows(&frozen, &renamed, "", &mut lost);
    assert!(lost.iter().any(|problem| problem.contains("description was removed")), "{lost:?}");
    assert!(problems.iter().any(|problem| problem.contains("changed whether it is required")));
}

#[test]
fn the_enums_of_the_document_are_the_ones_the_domain_writes() {
    use conta_verde_domain::{ChargeMethod, ChargeStatus, ProductKind, SaleStatus};
    let in_document = |schema: &str, field: &str, value: &str| {
        v1_schema(schema)["properties"][field]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!(value))
    };
    for method in [ChargeMethod::Boleto, ChargeMethod::Pix, ChargeMethod::CardLink] {
        assert!(in_document("Charge", "method", method.as_str()), "{method:?}");
    }
    for status in [ChargeStatus::Open, ChargeStatus::Paid, ChargeStatus::Cancelled, ChargeStatus::Expired] {
        assert!(in_document("Charge", "status", status.as_str()), "{status:?}");
    }
    for status in [SaleStatus::Open, SaleStatus::Completed, SaleStatus::Cancelled] {
        assert!(in_document("Sale", "status", status.as_str()), "{status:?}");
    }
    for kind in [ProductKind::Product, ProductKind::Service] {
        assert!(in_document("Product", "kind", kind.as_str()), "{kind:?}");
    }
}

#[tokio::test]
async fn keys_are_made_rotated_and_revoked_by_owners_and_admins_only() {
    let (app, identities) = entry_app_with_identities();
    let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
    let uri = format!("/companies/{company_id}/api-keys");

    // The secret is in the answer that makes the key and in no other.
    let made = send(
        &app,
        json_request(
            "POST",
            &uri,
            Some(&cookie),
            json!({ "name": "Loja", "scopes": ["sales:write", "sales:read", "parties:read"] }),
        ),
    )
    .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    let secret = made.body["secret"].as_str().unwrap().to_owned();
    assert!(secret.starts_with("cvk_") && made.body["key"]["label"] == secret[..10]);
    assert_eq!(made.body["key"]["scopes"], json!(["parties:read", "sales:write"]));
    assert_eq!(made.body["key"]["status"], "active");
    let key_id = made.body["key"]["id"].as_str().unwrap().to_owned();
    let listed = send(&app, get_request(&uri, Some(&cookie))).await;
    assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
    assert!(!listed.body.to_string().contains(&secret));

    // A scope a key may not have, and a body that says nothing, are refused.
    for scopes in [json!(["fiscal:read"]), json!(["members:write"]), json!([])] {
        let refused = send(
            &app,
            json_request("POST", &uri, Some(&cookie), json!({ "name": "x", "scopes": scopes })),
        )
        .await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{scopes}");
    }

    // A rotation makes a new secret and lets the old one go on for a while; a revocation is at once.
    let rotated = send(
        &app,
        json_request(
            "POST",
            &format!("{uri}/{key_id}/rotate"),
            Some(&cookie),
            json!({ "grace_hours": 2 }),
        ),
    )
    .await;
    assert_eq!(rotated.status, StatusCode::CREATED, "{}", rotated.body);
    assert_ne!(rotated.body["secret"], secret);
    assert_eq!(rotated.body["key"]["rotated_from"], key_id);
    let new_id = rotated.body["key"]["id"].as_str().unwrap().to_owned();
    let old = v1(&app, "GET", "/v1/parties", &secret, None).await;
    assert_eq!(old.status, StatusCode::OK);
    let revoked = send(
        &app,
        json_request("POST", &format!("{uri}/{new_id}/revoke"), Some(&cookie), json!({})),
    )
    .await;
    assert_eq!(revoked.body["status"], "revoked");
    let refused = v1(
        &app,
        "GET",
        "/v1/parties",
        rotated.body["secret"].as_str().unwrap(),
        None,
    )
    .await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);

    // The trail of a key says who did what, and never the secret.
    let history = send(&app, get_request(&format!("{uri}/{key_id}/history"), Some(&cookie))).await;
    assert_eq!(history.body.as_array().unwrap().len(), 2);
    assert_eq!(history.body[0]["actor"]["kind"], "user");
    assert!(!history.body.to_string().contains(&secret[4..]));

    // Only owners and admins reach the keys: the other roles are told there is nothing.
    for role in [
        conta_verde_domain::Role::Financial,
        conta_verde_domain::Role::Sales,
        conta_verde_domain::Role::Accountant,
        conta_verde_domain::Role::Viewer,
    ] {
        let member = member_with_role(&app, &identities, company_id, role).await;
        let list = send(&app, get_request(&uri, Some(&member))).await;
        assert_eq!(list.status, StatusCode::FORBIDDEN, "{role:?}");
        let create = send(
            &app,
            json_request("POST", &uri, Some(&member), json!({ "name": "x", "scopes": ["sales:read"] })),
        )
        .await;
        assert_eq!(create.status, StatusCode::FORBIDDEN, "{role:?}");
    }
    let admin = member_with_role(&app, &identities, company_id, conta_verde_domain::Role::Admin).await;
    assert_eq!(send(&app, get_request(&uri, Some(&admin))).await.status, StatusCode::OK);
    // Another company's key is nobody's.
    let (stranger, _) = sign_up(&app, "bia@example.com").await;
    let other = send(&app, get_request(&uri, Some(&stranger))).await;
    assert_eq!(other.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn v1_answers_in_problem_documents_and_asks_for_a_key_with_the_right_scope() {
    let (app, _) = entry_app_with_identities();
    let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
    let (read_only, read_only_id) = make_key(&app, &cookie, company_id, &["sales:read"]).await;

    // No key, a cookie in the place of one, and a key that is not one: all the same refusal.
    let none = raw(&app, Request::get("/v1/sales").body(Body::empty()).unwrap()).await;
    let with_cookie = raw(&app, get_request("/v1/sales", Some(&cookie))).await;
    for (status, headers, bytes) in [none, with_cookie] {
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(headers[header::CONTENT_TYPE], "application/problem+json");
        assert_eq!(headers[header::WWW_AUTHENTICATE], "Bearer");
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["code"], "unauthenticated");
        assert_eq!(body["status"], 401);
        assert_eq!(body["type"], "urn:conta-verde:problem:unauthenticated");
    }
    assert_eq!(
        v1(&app, "GET", "/v1/sales", "cvk_not-a-key", None).await.status,
        StatusCode::UNAUTHORIZED
    );
    // A bearer token does not open the web app's routes.
    let web = v1(&app, "GET", &format!("/companies/{company_id}/sales"), &read_only, None).await;
    assert_eq!(web.status, StatusCode::UNAUTHORIZED);

    // What the key may read it reads, with the allowance on the answer; what it may not is a 403 that says so.
    let sales = v1(&app, "GET", "/v1/sales", &read_only, None).await;
    assert_eq!(sales.status, StatusCode::OK);
    assert_v1_shape("SalePage", &sales.body);
    for name in ["ratelimit-limit", "ratelimit-remaining", "ratelimit-reset"] {
        assert!(sales.headers.contains_key(name), "{name}");
    }
    let body = json!({ "customer_id": Uuid::new_v4(), "sold_on": "2026-11-06", "category_id": Uuid::new_v4(), "lines": [] });
    for (method, uri, payload) in [
        ("POST", "/v1/sales", Some(body)),
        ("GET", "/v1/categories", None),
        ("GET", "/v1/parties", None),
        ("POST", "/v1/sales/00000000-0000-0000-0000-000000000000/cancel", None),
    ] {
        let reply = v1(&app, method, uri, &read_only, payload).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{method} {uri}");
        assert_eq!(reply.body["code"], "insufficient_scope");
        assert_eq!(reply.headers[header::CONTENT_TYPE], "application/problem+json");
    }

    // Every error of the use cases is a problem document too, with its own status.
    let unknown = v1(&app, "GET", "/v1/sales/not-an-id", &read_only, None).await;
    assert_eq!((unknown.status, unknown.body["code"].as_str()), (StatusCode::NOT_FOUND, Some("not_found")));
    let bad = v1(&app, "GET", "/v1/sales?status=nope", &read_only, None).await;
    assert_eq!((bad.status, bad.body["code"].as_str()), (StatusCode::BAD_REQUEST, Some("invalid_request")));
    let typo = v1(&app, "GET", "/v1/sales?stauts=open", &read_only, None).await;
    assert_eq!(typo.status, StatusCode::BAD_REQUEST);
    assert_eq!(typo.body["code"], "invalid_request");

    // The document is public and is the one that was checked in.
    let (status, _, bytes) = raw(&app, Request::get("/v1/openapi.json").body(Body::empty()).unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(String::from_utf8(bytes).unwrap(), V1_OPENAPI);

    // A revoked key is refused at once.
    let revoked = send(
        &app,
        json_request(
            "POST",
            &format!("/companies/{company_id}/api-keys/{read_only_id}/revoke"),
            Some(&cookie),
            json!({}),
        ),
    )
    .await;
    assert_eq!(revoked.status, StatusCode::OK);
    assert_eq!(v1(&app, "GET", "/v1/sales", &read_only, None).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_key_expires_and_is_limited_by_the_clock_and_the_rate_the_operator_set() {
    use std::sync::atomic::{AtomicI64, Ordering};
    let now = std::sync::Arc::new(AtomicI64::new(1_800_000_000_000));
    let clock = {
        let now = now.clone();
        V1Clock::of(move || now.load(Ordering::SeqCst))
    };
    let (app, _) = entry_app_over_v1(
        InMemoryIdentityRepository::default(),
        V1Config {
            rate_limit_per_minute: 3,
            clock,
        },
    );
    let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
    // A key that lives for an hour, made through the web app (whose own clock is the server's).
    let (secret, _) = make_key(&app, &cookie, company_id, &["sales:read"]).await;

    // Three requests a minute: the fourth is told how long to wait, with the allowance at 0.
    for remaining in [2, 1, 0] {
        let reply = v1(&app, "GET", "/v1/sales", &secret, None).await;
        assert_eq!(reply.status, StatusCode::OK);
        assert_eq!(reply.headers["ratelimit-limit"], "3");
        assert_eq!(reply.headers["ratelimit-remaining"], remaining.to_string());
    }
    let refused = v1(&app, "GET", "/v1/sales", &secret, None).await;
    assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(refused.body["code"], "rate_limited");
    assert_eq!(refused.headers["retry-after"], "20");
    assert_eq!(refused.headers["ratelimit-remaining"], "0");
    // A token comes back every 20 seconds.
    now.fetch_add(20_000, Ordering::SeqCst);
    assert_eq!(v1(&app, "GET", "/v1/sales", &secret, None).await.status, StatusCode::OK);
    assert_eq!(v1(&app, "GET", "/v1/sales", &secret, None).await.status, StatusCode::TOO_MANY_REQUESTS);
    // The openapi document and a refusal of the key itself are not counted against it.
    assert_eq!(
        raw(&app, Request::get("/v1/openapi.json").body(Body::empty()).unwrap()).await.0,
        StatusCode::OK
    );

    // A key that lives an hour is refused once the clock runs past it. The web app made it with the real clock, so the
    // test clock goes years beyond.
    let short = send(
        &app,
        json_request(
            "POST",
            &format!("/companies/{company_id}/api-keys"),
            Some(&cookie),
            json!({ "name": "Curta", "scopes": ["sales:read"], "expires_in_hours": 1 }),
        ),
    )
    .await;
    let short = short.body["secret"].as_str().unwrap().to_owned();
    now.fetch_add(10 * 365 * 24 * 3_600_000, Ordering::SeqCst);
    assert_eq!(v1(&app, "GET", "/v1/sales", &short, None).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_third_party_makes_a_sale_once_over_v1_and_it_is_the_keys_in_the_trail() {
    let (app, _) = entry_app_with_identities();
    let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
    let targets = sales_targets(&app, &cookie, company_id).await;
    let category = revenue_category(&app, &cookie, company_id).await;
    let (secret, key_id) = make_key(
        &app,
        &cookie,
        company_id,
        &["sales:write", "parties:write", "products:read", "finance:read", "purchases:read"],
    )
    .await;

    // What the order names is found through the same API.
    let products = v1(&app, "GET", "/v1/products", &secret, None).await;
    assert_eq!(products.status, StatusCode::OK, "{}", products.body);
    assert_v1_shape("ProductPage", &products.body);
    let product = products.body["items"][0]["id"].as_str().unwrap().to_owned();
    let product_reply = v1(&app, "GET", &format!("/v1/products/{product}"), &secret, None).await;
    assert_v1_shape("Product", &product_reply.body);
    let stock = v1(&app, "GET", &format!("/v1/products/{product}/stock"), &secret, None).await;
    assert_eq!(stock.status, StatusCode::OK, "{}", stock.body);
    assert_v1_shape("Stock", &stock.body);
    let categories = v1(&app, "GET", "/v1/categories", &secret, None).await;
    assert_v1_shape("CategoryList", &categories.body);
    assert!(categories.body["items"].as_array().unwrap().iter().any(|c| c["id"] == category));
    let terms = v1(&app, "GET", "/v1/payment-terms", &secret, None).await;
    assert_v1_shape("PaymentTermPage", &terms.body);

    // A party is registered and found again; the same document is refused.
    let party = json!({ "name": "Cliente da loja", "document": "529.982.247-25", "email": "cliente@example.com",
        "is_customer": true, "is_supplier": false });
    let made = v1(&app, "POST", "/v1/parties", &secret, Some(party.clone())).await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    assert_v1_shape("Party", &made.body);
    assert_eq!(v1(&app, "POST", "/v1/parties", &secret, Some(party)).await.status, StatusCode::CONFLICT);
    let party_id = made.body["id"].as_str().unwrap();
    assert_eq!(v1(&app, "GET", &format!("/v1/parties/{party_id}"), &secret, None).await.status, StatusCode::OK);
    let found = v1(&app, "GET", "/v1/parties?q=Cliente", &secret, None).await;
    assert_v1_shape("PartyPage", &found.body);
    assert_eq!(found.body["items"].as_array().unwrap().len(), 1);

    // The order is made with a reference: sent twice it is one sale, and another request under it is refused.
    let mut order = json!({
        "external_reference": "loja-1042",
        "customer_id": targets.customer,
        "sold_on": "2026-11-06",
        "payment_term_id": targets.term,
        "category_id": category,
        "notes": "Pedido 1042",
        "lines": [{ "product_id": targets.product, "description": "Café 500g", "quantity": "3", "unit_price": "10.10" }],
    });
    let first = v1(&app, "POST", "/v1/sales", &secret, Some(order.clone())).await;
    assert_eq!(first.status, StatusCode::CREATED, "{}", first.body);
    assert_v1_shape("Sale", &first.body);
    assert_eq!((first.body["status"].as_str(), first.body["external_reference"].as_str()), (Some("open"), Some("loja-1042")));
    let sale_id = first.body["id"].as_str().unwrap().to_owned();
    let again = v1(&app, "POST", "/v1/sales", &secret, Some(order.clone())).await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.body);
    assert_eq!(again.body["id"], first.body["id"]);
    order["notes"] = json!("Outro pedido");
    let changed = v1(&app, "POST", "/v1/sales", &secret, Some(order)).await;
    assert_eq!(changed.status, StatusCode::CONFLICT);
    assert_eq!(changed.body["code"], "conflict");
    let by_reference = v1(&app, "GET", "/v1/sales?external_reference=loja-1042", &secret, None).await;
    assert_eq!(by_reference.body["items"].as_array().unwrap().len(), 1);
    assert_eq!(by_reference.body["items"][0]["id"], sale_id);
    let none = v1(&app, "GET", "/v1/sales?external_reference=unknown", &secret, None).await;
    assert!(none.body["items"].as_array().unwrap().is_empty());
    let listed = v1(&app, "GET", "/v1/sales", &secret, None).await;
    assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
    assert_v1_shape("SalePage", &listed.body);

    // A rule of the use case is a 422 in a problem document, and nothing was made.
    let refused = v1(
        &app,
        "POST",
        "/v1/sales",
        &secret,
        Some(json!({ "customer_id": Uuid::new_v4(), "sold_on": "2026-11-06", "category_id": category,
            "lines": [{ "product_id": targets.product, "description": "x", "quantity": "1", "unit_price": "1" }] })),
    )
    .await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", refused.body);
    assert_eq!(refused.body["code"], "invalid_request");
    // What a rule refuses does not take the reference: the same reference with a request that was fixed makes the sale.
    let mut broken = json!({ "external_reference": "loja-1099", "customer_id": Uuid::new_v4(), "sold_on": "2026-11-06",
        "category_id": category, "lines": [{ "product_id": targets.product, "description": "x", "quantity": "1", "unit_price": "1" }] });
    assert_eq!(v1(&app, "POST", "/v1/sales", &secret, Some(broken.clone())).await.status, StatusCode::UNPROCESSABLE_ENTITY);
    broken["customer_id"] = json!(targets.customer);
    let fixed = v1(&app, "POST", "/v1/sales", &secret, Some(broken)).await;
    assert_eq!(fixed.status, StatusCode::CREATED, "{}", fixed.body);
    // A reference lookup takes no other filter, and a bad one is not an empty answer.
    let combined = v1(&app, "GET", "/v1/sales?external_reference=loja-1099&status=open", &secret, None).await;
    assert_eq!(combined.status, StatusCode::BAD_REQUEST);
    // A decimal sent as a number is refused: decimals travel as strings.
    let number = v1(
        &app,
        "POST",
        "/v1/sales",
        &secret,
        Some(json!({ "customer_id": targets.customer, "sold_on": "2026-11-06", "category_id": category,
            "lines": [{ "product_id": targets.product, "description": "x", "quantity": 1, "unit_price": "1" }] })),
    )
    .await;
    assert!(number.status.is_client_error());
    assert_eq!(number.headers[header::CONTENT_TYPE], "application/problem+json");

    // It is completed into a receivable, and asking again changes nothing.
    let completed = v1(&app, "POST", &format!("/v1/sales/{sale_id}/complete"), &secret, None).await;
    assert_eq!(completed.status, StatusCode::OK, "{}", completed.body);
    assert_v1_shape("Sale", &completed.body);
    assert_eq!(completed.body["status"], "completed");
    assert!(completed.body["receivable_id"].is_string());
    let repeated = v1(&app, "POST", &format!("/v1/sales/{sale_id}/complete"), &secret, None).await;
    assert_eq!(repeated.body["receivable_id"], completed.body["receivable_id"]);
    let charges = v1(&app, "GET", &format!("/v1/sales/{sale_id}/charges"), &secret, None).await;
    assert_eq!(charges.status, StatusCode::OK, "{}", charges.body);
    assert_v1_shape("ChargeList", &charges.body);
    let got = v1(&app, "GET", &format!("/v1/sales/{sale_id}"), &secret, None).await;
    assert_v1_shape("Sale", &got.body);

    // The trail names the key, not a person, and a company's key cannot reach another company's sale.
    let history = send(
        &app,
        get_request(&format!("/companies/{company_id}/sales/{sale_id}/history"), Some(&cookie)),
    )
    .await;
    let actors: Vec<(&str, &str, &str)> = history
        .body
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["actor"]["kind"].as_str().unwrap(),
                entry["actor"]["id"].as_str().unwrap(),
                entry["actor"]["name"].as_str().unwrap(),
            )
        })
        .collect();
    assert!(!actors.is_empty());
    assert!(actors.iter().all(|actor| *actor == ("api_key", key_id.as_str(), "Loja online")), "{actors:?}");
    let (stranger, stranger_company) = sign_up(&app, "bia@example.com").await;
    let (theirs, _) = make_key(&app, &stranger, stranger_company, &["sales:write"]).await;
    let hidden = v1(&app, "GET", &format!("/v1/sales/{sale_id}"), &theirs, None).await;
    assert_eq!((hidden.status, hidden.body["code"].as_str()), (StatusCode::NOT_FOUND, Some("not_found")));
    let cancel = v1(&app, "POST", &format!("/v1/sales/{sale_id}/cancel"), &theirs, None).await;
    assert_eq!(cancel.status, StatusCode::NOT_FOUND);
    let reference = v1(&app, "GET", "/v1/sales?external_reference=loja-1042", &theirs, None).await;
    assert!(reference.body["items"].as_array().unwrap().is_empty());
    // A key that cannot reach finance is told so for the charges of a sale it can read.
    let (sales_only, _) = make_key(&app, &cookie, company_id, &["sales:write"]).await;
    let forbidden = v1(&app, "GET", &format!("/v1/sales/{sale_id}/charges"), &sales_only, None).await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);
    // And the sale is cancelled, with its receivable, through the same door.
    let cancelled = v1(&app, "POST", &format!("/v1/sales/{sale_id}/cancel"), &sales_only, None).await;
    assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.body);
    assert_eq!(cancelled.body["status"], "cancelled");
}
