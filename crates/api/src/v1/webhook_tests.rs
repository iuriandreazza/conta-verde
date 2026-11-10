// Included in the `tests` module of `lib.rs` after `tests.rs`: the webhooks of the public API ([ADR 0088]).

/// What a subscriber's listener received.
#[derive(Clone, Debug)]
struct Received {
    headers: std::collections::HashMap<String, String>,
    body: String,
}

/// A listener on this machine that records every request and answers with `status`, the way a subscriber would.
async fn listen(status: u16) -> (String, Arc<std::sync::Mutex<Vec<Received>>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    let received = Arc::new(std::sync::Mutex::new(Vec::new()));
    let keep = received.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else { return };
            let keep = keep.clone();
            tokio::spawn(async move {
                let mut bytes = Vec::new();
                let mut chunk = [0u8; 4096];
                let (head_end, length) = loop {
                    let Ok(n) = socket.read(&mut chunk).await else { return };
                    if n == 0 {
                        return;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let length = head
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length:"))
                            .and_then(|value| value.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        break (end + 4, length);
                    }
                };
                while bytes.len() < head_end + length {
                    let Ok(n) = socket.read(&mut chunk).await else { return };
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                }
                let head = String::from_utf8_lossy(&bytes[..head_end]).to_string();
                let headers = head
                    .lines()
                    .skip(1)
                    .filter_map(|line| line.split_once(':'))
                    .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_owned()))
                    .collect();
                let body = String::from_utf8_lossy(&bytes[head_end..]).to_string();
                keep.lock().unwrap().push(Received { headers, body });
                let reply = format!("HTTP/1.1 {status} X\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
                let _ = socket.write_all(reply.as_bytes()).await;
            });
        }
    });
    (url, received)
}

fn hooks_of_this_app() -> Arc<TestHooks> {
    HOOKS.with(|held| held.borrow().clone().expect("the app of this test has webhooks"))
}

/// Does the deliveries that are due, as the worker of the server would.
async fn send_due(sender: &impl conta_verde_application::WebhookSender) {
    let now = crate::clock::now_millis() / 1000 + 5;
    hooks_of_this_app().run_due(now, 50, sender).await.unwrap();
}

async fn subscribe(app: &Router, cookie: &str, company_id: Uuid, url: &str, events: Value) -> (String, String) {
    let reply = send(
        app,
        json_request(
            "POST",
            &format!("/companies/{company_id}/webhook-subscriptions"),
            Some(cookie),
            json!({ "url": url, "events": events }),
        ),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    (
        reply.body["subscription"]["id"].as_str().unwrap().to_owned(),
        reply.body["secret"].as_str().unwrap().to_owned(),
    )
}

#[tokio::test]
async fn subscriptions_are_managed_by_owners_and_admins_and_their_secret_is_shown_once_or_typed() {
    let (app, identities) = entry_app_with_identities();
    let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
    let uri = format!("/companies/{company_id}/webhook-subscriptions");

    let made = send(
        &app,
        json_request(
            "POST",
            &uri,
            Some(&cookie),
            json!({ "url": "https://hooks.example.com/x", "events": ["sale.completed", "charge.paid"] }),
        ),
    )
    .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    let secret = made.body["secret"].as_str().unwrap().to_owned();
    assert!(secret.starts_with("whsec_"));
    assert_eq!(made.body["subscription"]["events"], json!(["charge.paid", "sale.completed"]));
    assert_eq!(made.body["subscription"]["active"], true);
    let id = made.body["subscription"]["id"].as_str().unwrap().to_owned();

    // It lists, with what can be asked for and whether the server can seal secrets; the secret is nowhere in it.
    let listed = send(&app, get_request(&uri, Some(&cookie))).await;
    assert_eq!(listed.body["items"].as_array().unwrap().len(), 1);
    assert_eq!(listed.body["available"], true);
    assert_eq!(listed.body["events"].as_array().unwrap().len(), 5);
    assert!(!listed.body.to_string().contains(&secret[6..]));

    // What is refused: an address that is not http(s), an event that does not exist, a body that lacks what replacing needs.
    for body in [
        json!({ "url": "ftp://hooks.example.com/x", "events": [] }),
        json!({ "url": "https://hooks.example.com/x", "events": ["sale.sold"] }),
        json!({ "url": "https://hooks.example.com/x", "events": ["webhook.test"] }),
    ] {
        let refused = send(&app, json_request("POST", &uri, Some(&cookie), body.clone())).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }
    let no_active = send(
        &app,
        json_request("PUT", &format!("{uri}/{id}"), Some(&cookie), json!({ "url": "https://hooks.example.com/x", "events": [] })),
    )
    .await;
    assert_eq!(no_active.status, StatusCode::UNPROCESSABLE_ENTITY);
    let changed = send(
        &app,
        json_request(
            "PUT",
            &format!("{uri}/{id}"),
            Some(&cookie),
            json!({ "url": "https://hooks.example.com/y", "events": [], "active": false }),
        ),
    )
    .await;
    assert_eq!((changed.body["active"].as_bool(), changed.body["events"].clone()), (Some(false), json!([])));

    // The secret is typed (and never given back) or made again (and shown now).
    let typed = send(
        &app,
        json_request("POST", &format!("{uri}/{id}/secret"), Some(&cookie), json!({ "secret": "my-own-secret-123" })),
    )
    .await;
    assert_eq!((typed.status, typed.body["secret"].clone()), (StatusCode::OK, Value::Null));
    let short = send(
        &app,
        json_request("POST", &format!("{uri}/{id}/secret"), Some(&cookie), json!({ "secret": "short" })),
    )
    .await;
    assert_eq!(short.status, StatusCode::UNPROCESSABLE_ENTITY);
    let made_again = send(&app, json_request("POST", &format!("{uri}/{id}/secret"), Some(&cookie), json!({}))).await;
    assert!(made_again.body["secret"].as_str().unwrap().starts_with("whsec_"));

    // A test event is sent also to a subscription that is off, and is in the log; so is a delivery sent again by hand.
    let test = send(&app, json_request("POST", &format!("{uri}/{id}/test"), Some(&cookie), json!({}))).await;
    assert_eq!(test.status, StatusCode::ACCEPTED, "{}", test.body);
    assert_eq!((test.body["kind"].as_str(), test.body["status"].as_str()), (Some("webhook.test"), Some("pending")));
    let sender = conta_verde_infrastructure::InMemoryWebhookSender::default();
    send_due(&sender).await;
    assert_eq!(sender.sent().len(), 1);
    let log = send(&app, get_request(&format!("{uri}/{id}/deliveries"), Some(&cookie))).await;
    assert_eq!(log.body["items"][0]["status"], "delivered");
    let delivery_id = log.body["items"][0]["id"].as_str().unwrap().to_owned();
    let again = send(
        &app,
        json_request("POST", &format!("{uri}/{id}/deliveries/{delivery_id}/redeliver"), Some(&cookie), json!({})),
    )
    .await;
    assert_eq!((again.status, again.body["attempts"].clone()), (StatusCode::ACCEPTED, json!(0)));
    send_due(&sender).await;
    assert_eq!(sender.sent().len(), 2);
    let unknown = send(
        &app,
        json_request("POST", &format!("{uri}/{id}/deliveries/{}/redeliver", Uuid::new_v4()), Some(&cookie), json!({})),
    )
    .await;
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    let bad = send(&app, get_request(&format!("{uri}/{id}/deliveries?after=nope"), Some(&cookie))).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    // The trail names what changed, never a secret.
    let history = send(&app, get_request(&format!("{uri}/{id}/history"), Some(&cookie))).await;
    let text = history.body.to_string();
    assert!(history.body.as_array().unwrap().len() >= 4);
    assert!(text.contains("\"secret\"") && !text.contains("my-own-secret-123") && !text.contains(&secret[6..]));

    // Only owners and admins reach them, and another company's is nobody's.
    for role in [
        conta_verde_domain::Role::Financial,
        conta_verde_domain::Role::Sales,
        conta_verde_domain::Role::Accountant,
        conta_verde_domain::Role::Viewer,
    ] {
        let member = member_with_role(&app, &identities, company_id, role).await;
        assert_eq!(send(&app, get_request(&uri, Some(&member))).await.status, StatusCode::FORBIDDEN, "{role:?}");
        let create = send(
            &app,
            json_request("POST", &uri, Some(&member), json!({ "url": "https://hooks.example.com/z", "events": [] })),
        )
        .await;
        assert_eq!(create.status, StatusCode::FORBIDDEN, "{role:?}");
    }
    let (stranger, stranger_company) = sign_up(&app, "bia@example.com").await;
    assert_eq!(send(&app, get_request(&uri, Some(&stranger))).await.status, StatusCode::NOT_FOUND);
    let theirs = format!("/companies/{stranger_company}/webhook-subscriptions/{id}");
    assert_eq!(send(&app, json_request("PUT", &theirs, Some(&stranger), json!({ "url": "https://hooks.example.com/x", "events": [], "active": true }))).await.status, StatusCode::NOT_FOUND);
    assert_eq!(send(&app, json_request("POST", &format!("{theirs}/test"), Some(&stranger), json!({}))).await.status, StatusCode::NOT_FOUND);
}

/// The exit criterion of Phase 8: a third-party client makes a sale over `/v1` with a key, and its webhook arrives at the
/// client's own listener, signed, without any endpoint of the web app but the one the person used to subscribe.
#[tokio::test]
async fn a_third_party_makes_a_sale_over_v1_and_receives_its_signed_webhooks() {
    let (app, _) = entry_app_with_identities();
    let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
    let targets = sales_targets(&app, &cookie, company_id).await;
    let category = revenue_category(&app, &cookie, company_id).await;

    // The company subscribes the client's listener, with a secret the client knows.
    let (url, received) = listen(200).await;
    let (subscription, _) = subscribe(&app, &cookie, company_id, &url, json!(["sale.created", "sale.completed", "sale.cancelled"])).await;
    let secret = "client-knows-this-secret";
    let typed = send(
        &app,
        json_request(
            "POST",
            &format!("/companies/{company_id}/webhook-subscriptions/{subscription}/secret"),
            Some(&cookie),
            json!({ "secret": secret }),
        ),
    )
    .await;
    assert_eq!(typed.status, StatusCode::OK);

    // A key that may only read cannot make the sale, and nothing is told about what did not happen.
    let (read_only, _) = make_key(&app, &cookie, company_id, &["sales:read"]).await;
    let order = json!({
        "external_reference": "loja-9001",
        "customer_id": targets.customer,
        "sold_on": "2026-11-06",
        "payment_term_id": targets.term,
        "category_id": category,
        "lines": [{ "product_id": targets.product, "description": "Café 500g", "quantity": "3", "unit_price": "10.10" }],
    });
    assert_eq!(v1(&app, "POST", "/v1/sales", &read_only, Some(order.clone())).await.status, StatusCode::FORBIDDEN);

    // The client, with a key of its own, makes the sale over /v1 and completes it. Nothing private is used.
    let (key, _) = make_key(&app, &cookie, company_id, &["sales:write"]).await;
    let made = v1(&app, "POST", "/v1/sales", &key, Some(order.clone())).await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    let sale_id = made.body["id"].as_str().unwrap().to_owned();
    // Sending the order again makes no sale and tells nobody anything new.
    assert_eq!(v1(&app, "POST", "/v1/sales", &key, Some(order)).await.status, StatusCode::OK);
    let completed = v1(&app, "POST", &format!("/v1/sales/{sale_id}/complete"), &key, None).await;
    assert_eq!(completed.status, StatusCode::OK, "{}", completed.body);

    // The worker sends what is due; the listener gets one request for each event, and only those.
    send_due(&conta_verde_infrastructure::HttpWebhookSender::new(true)).await;
    let got = received.lock().unwrap().clone();
    assert_eq!(got.len(), 2, "{got:?}");
    let mut kinds = Vec::new();
    for request in &got {
        let event: Value = serde_json::from_str(&request.body).unwrap();
        kinds.push(event["type"].as_str().unwrap().to_owned());
        // It has the shape the document promises, and is about the sale the client made.
        assert_v1_shape("WebhookEvent", &event);
        assert_eq!(event["company_id"], company_id.to_string());
        assert_eq!(event["data"]["object"]["id"], sale_id);
        assert_eq!(event["data"]["object"]["external_reference"], "loja-9001");
        assert_v1_shape("Sale", &event["data"]["object"]);

        // It is signed with the secret the client has: id, timestamp and body are what was signed, and nothing else verifies.
        let id = &request.headers["webhook-id"];
        assert_eq!(id, event["id"].as_str().unwrap());
        let timestamp: i64 = request.headers["webhook-timestamp"].parse().unwrap();
        assert!((crate::clock::now_millis() / 1000 + 5 - timestamp).abs() < 30);
        let signature = &request.headers["webhook-signature"];
        // The secret that was replaced still signs for a day, so there are two values: the client's is one of them.
        let values: Vec<&str> = signature.split(' ').collect();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0], conta_verde_infrastructure::webhook_signature(secret, id, timestamp, &request.body));
        for wrong in [
            conta_verde_infrastructure::webhook_signature("another-secret!", id, timestamp, &request.body),
            conta_verde_infrastructure::webhook_signature(secret, id, timestamp + 1, &request.body),
            conta_verde_infrastructure::webhook_signature(secret, id, timestamp, &format!("{} ", request.body)),
        ] {
            assert!(!values.contains(&wrong.as_str()));
        }
        assert_eq!(request.headers["content-type"], "application/json");
    }
    kinds.sort();
    assert_eq!(kinds, ["sale.completed", "sale.created"]);

    // The log says both were delivered, once each; and a sale cancelled later is a new event, told once.
    let log = send(
        &app,
        get_request(&format!("/companies/{company_id}/webhook-subscriptions/{subscription}/deliveries"), Some(&cookie)),
    )
    .await;
    let items = log.body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|d| d["status"] == "delivered" && d["attempts"] == 1 && d["last_status"] == 200));
    assert_eq!(v1(&app, "POST", &format!("/v1/sales/{sale_id}/cancel"), &key, None).await.status, StatusCode::OK);
    send_due(&conta_verde_infrastructure::HttpWebhookSender::new(true)).await;
    send_due(&conta_verde_infrastructure::HttpWebhookSender::new(true)).await;
    let got = received.lock().unwrap().clone();
    assert_eq!(got.len(), 3);
    assert!(got[2].body.contains("\"type\":\"sale.cancelled\""));

    // Another company's key reaches none of it, and another company hears nothing of this one.
    let (stranger, stranger_company) = sign_up(&app, "bia@example.com").await;
    let (theirs, _) = make_key(&app, &stranger, stranger_company, &["sales:write"]).await;
    assert_eq!(v1(&app, "GET", &format!("/v1/sales/{sale_id}"), &theirs, None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(received.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn a_subscriber_that_is_down_is_tried_again_and_the_log_says_why() {
    let (app, _) = entry_app_with_identities();
    let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
    let targets = sales_targets(&app, &cookie, company_id).await;
    let category = revenue_category(&app, &cookie, company_id).await;
    let (url, received) = listen(503).await;
    let (subscription, _) = subscribe(&app, &cookie, company_id, &url, json!(["sale.created"])).await;
    let (key, _) = make_key(&app, &cookie, company_id, &["sales:write"]).await;
    let order = json!({ "customer_id": targets.customer, "sold_on": "2026-11-06", "category_id": category,
        "lines": [{ "product_id": targets.product, "description": "x", "quantity": "1", "unit_price": "1" }] });
    assert_eq!(v1(&app, "POST", "/v1/sales", &key, Some(order)).await.status, StatusCode::CREATED);
    send_due(&conta_verde_infrastructure::HttpWebhookSender::new(true)).await;
    assert_eq!(received.lock().unwrap().len(), 1);
    let log = send(
        &app,
        get_request(&format!("/companies/{company_id}/webhook-subscriptions/{subscription}/deliveries"), Some(&cookie)),
    )
    .await;
    let delivery = &log.body["items"][0];
    assert_eq!((delivery["status"].as_str(), delivery["attempts"].as_u64(), delivery["last_status"].as_u64()), (Some("pending"), Some(1), Some(503)));
    assert_eq!(delivery["last_error"], "the subscriber answered 503");
    // It waits a minute before it is tried again.
    send_due(&conta_verde_infrastructure::HttpWebhookSender::new(true)).await;
    assert_eq!(received.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_charge_that_is_paid_is_told_once_with_its_receivable() {
    let app = entry_app();
    let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
    let targets = entry_targets(&app, &cookie, company_id).await;
    configure_collection(&app, &cookie, company_id, Some(&targets.other_category)).await;
    let sender = conta_verde_infrastructure::InMemoryWebhookSender::default();
    subscribe(&app, &cookie, company_id, "http://127.0.0.1:9/hook", json!(["charge.paid"])).await;
    let created = send(
        &app,
        json_request("POST", &entries_uri(company_id), Some(&cookie), entry_body(&targets, "Venda", &[("2026-12-10", "100")])),
    )
    .await;
    let entry_id = created.body["id"].as_str().unwrap().to_owned();
    let issued = send(
        &app,
        json_request(
            "POST",
            &format!("/companies/{company_id}/entries/{entry_id}/charges"),
            Some(&cookie),
            json!({ "installment": 1, "method": "pix" }),
        ),
    )
    .await;
    let external = format!("fake_{}", issued.body["id"].as_str().unwrap().replace('-', ""));
    let hook = format!("/public/payments/{company_id}/webhooks/fake");
    let (signature, body) = signed_notice("evt_1", &external, "paid", Some(("100", "2026-12-09", "1.99")));
    // Told twice by the provider, paid once, told once.
    for _ in 0..2 {
        assert_eq!(send(&app, webhook_request(&hook, Some(&signature), body.clone())).await.status, StatusCode::OK);
    }
    send_due(&sender).await;
    let sent = sender.sent();
    assert_eq!(sent.len(), 1);
    let event: Value = serde_json::from_str(&sent[0].body).unwrap();
    assert_eq!(event["type"], "charge.paid");
    assert_eq!(event["data"]["object"]["receivable_id"], entry_id);
    assert!(event["data"]["object"]["sale_id"].is_null());
    assert_eq!(event["data"]["object"]["charge"]["status"], "paid");
    assert_eq!(event["data"]["object"]["charge"]["paid_amount"], "100");
    assert_v1_shape("WebhookEvent", &event);
    assert_v1_shape("ChargePaid", &event["data"]["object"]);
}

#[tokio::test]
async fn stock_below_its_minimum_is_told_when_a_movement_crosses_it_and_not_otherwise() {
    use conta_verde_application::{DomainEvent, EventPublisher};
    use conta_verde_domain::{Decimal, ProductDraft, ProductId};
    let products = InMemoryProductRepository::default();
    let company = conta_verde_domain::CompanyId::generate();
    let ana = conta_verde_domain::Actor::User {
        id: conta_verde_domain::UserId::generate(),
        name: "Ana".into(),
    };
    let product = ProductService::new(products.clone())
        .register(
            company,
            &ana,
            ProductDraft {
                kind: conta_verde_domain::ProductKind::Product,
                name: "Café".into(),
                sku: Some("CAFE".into()),
                unit: "UN".into(),
                ncm: None,
                sale_price: None,
                cost_price: None,
                minimum_stock: Some("5".into()),
                active: true,
            },
        )
        .await
        .unwrap();
    let hooks = standalone_hooks();
    hooks
        .create(
            company,
            &ana,
            conta_verde_domain::WebhookSubscriptionDraft {
                url: "https://hooks.example.com/x".into(),
                events: vec!["stock.below_minimum".into()],
                active: true,
            },
            0,
        )
        .await
        .unwrap();
    let events = V1Events::new(
        hooks.clone(),
        InMemorySaleClaimRepository::default(),
        InMemoryFinancialEntryRepository::default(),
        products,
        V1Clock::of(|| 1_800_000_000_000),
    );
    let moved = |before: i64, after: i64| DomainEvent::StockMoved {
        company_id: company,
        product_id: ProductId::from_uuid(product.id().as_uuid()),
        movement_id: Uuid::new_v4(),
        before: Decimal::from(before),
        after: Decimal::from(after),
    };
    // Above it, still above it, already below it and still below it, and going up: none is news.
    for (before, after) in [(10, 7), (7, 5), (4, 3), (3, 2), (2, 9)] {
        events.publish(moved(before, after)).await;
    }
    let sender = conta_verde_infrastructure::InMemoryWebhookSender::default();
    hooks.run_due(1_800_000_100, 50, &sender).await.unwrap();
    assert!(sender.sent().is_empty());
    // From the minimum or more to less than it: the one that is.
    events.publish(moved(5, 4)).await;
    hooks.run_due(1_800_000_100, 50, &sender).await.unwrap();
    let sent = sender.sent();
    assert_eq!(sent.len(), 1);
    let event: Value = serde_json::from_str(&sent[0].body).unwrap();
    assert_eq!(event["type"], "stock.below_minimum");
    assert_eq!(event["data"]["object"]["quantity"], "4");
    assert_eq!(event["data"]["object"]["minimum_stock"], "5");
    assert_eq!(event["data"]["object"]["product_id"], product.id().as_uuid().to_string());
    assert_v1_shape("WebhookEvent", &event);
}
