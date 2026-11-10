// Included in the `tests` module of `lib.rs` after the webhook tests: the bank connections ([ADR 0089]).

fn this_aggregator() -> FakeBankAggregator {
    AGGREGATOR.with(|held| held.borrow().clone())
}

#[tokio::test]
async fn a_bank_account_is_connected_and_its_statement_comes_in_once_as_lines_to_reconcile() {
    let (app, _) = entry_app_with_identities();
    let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
    let account = registered_account(&app, &cookie, company_id, "Caixa").await;
    let uri = format!("/companies/{company_id}/bank-connections");
    let aggregator = this_aggregator().with_account("item-1", "acc-1", "Conta Corrente");
    let today = crate::clock::today();
    let recent = today.to_string();
    aggregator.add_transaction("acc-1", "tx-1", &recent, "1250.50", "PIX RECEBIDO Maria");
    aggregator.add_transaction("acc-1", "tx-2", &recent, "-30", "TARIFA");
    aggregator.add_transaction("acc-1", "tx-3", &recent, "0", "ZERADO");

    // Before anything: what the server has, a token for the widget, and the accounts a connection holds.
    let listed = send(&app, get_request(&uri, Some(&cookie))).await;
    assert_eq!(
        (listed.body["available"].as_bool(), listed.body["provider"].as_str(), listed.body["items"].as_array().unwrap().len()),
        (Some(true), Some("fake"), 0)
    );
    let token = send(&app, json_request("POST", &format!("{uri}/connect-token"), Some(&cookie), json!({}))).await;
    assert_eq!(token.status, StatusCode::OK, "{}", token.body);
    assert!(token.body["access_token"].as_str().unwrap().starts_with("fake-token-"));
    let accounts = send(&app, get_request(&format!("{uri}/accounts?item_id=item-1"), Some(&cookie))).await;
    assert_eq!(accounts.body["items"][0]["id"], "acc-1");
    let unknown = send(&app, get_request(&format!("{uri}/accounts?item_id=nope"), Some(&cookie))).await;
    assert_eq!(unknown.status, StatusCode::BAD_GATEWAY);
    assert!(!unknown.body.to_string().contains("no such item"));

    // What is refused: a bank account that is not the company's, an account the item does not hold.
    for body in [
        json!({ "bank_account_id": Uuid::new_v4(), "item_id": "item-1", "account_id": "acc-1" }),
        json!({ "bank_account_id": account, "item_id": "item-1", "account_id": "acc-x" }),
    ] {
        let refused = send(&app, json_request("POST", &uri, Some(&cookie), body.clone())).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }
    let made = send(
        &app,
        json_request("POST", &uri, Some(&cookie), json!({ "bank_account_id": account, "item_id": "item-1", "account_id": "acc-1" })),
    )
    .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    assert_eq!((made.body["status"].as_str(), made.body["provider"].as_str(), made.body["last_synced_on"].clone()), (Some("connected"), Some("fake"), Value::Null));
    let id = made.body["id"].as_str().unwrap().to_owned();
    let twice = send(
        &app,
        json_request("POST", &uri, Some(&cookie), json!({ "bank_account_id": account, "item_id": "item-1", "account_id": "acc-1" })),
    )
    .await;
    assert_eq!(twice.status, StatusCode::CONFLICT);

    // The statement comes in as the lines reconciliation works on, once however many times it is pulled.
    let synced = send(&app, json_request("POST", &format!("{uri}/{id}/sync"), Some(&cookie), json!({}))).await;
    assert_eq!(synced.status, StatusCode::OK, "{}", synced.body);
    assert_eq!(
        (synced.body["created"].as_u64(), synced.body["skipped"].as_u64(), synced.body["refused"].as_u64(), synced.body["needs_attention"].as_bool()),
        (Some(2), Some(0), Some(1), Some(false))
    );
    let lines = send(&app, get_request(&lines_uri(company_id), Some(&cookie))).await;
    let items = lines.body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|line| line["amount"] == "1250.5" || line["amount"] == "1250.50"), "{items:?}");
    let again = send(&app, json_request("POST", &format!("{uri}/{id}/sync"), Some(&cookie), json!({}))).await;
    assert_eq!((again.body["created"].as_u64(), again.body["skipped"].as_u64()), (Some(0), Some(2)));
    assert_eq!(send(&app, get_request(&lines_uri(company_id), Some(&cookie))).await.body["items"].as_array().unwrap().len(), 2);
    let after = send(&app, get_request(&uri, Some(&cookie))).await;
    assert_eq!(after.body["items"][0]["last_synced_on"], recent);

    // The same lines are reconciled as any: and an OFX file for the same account adds to them as before.
    let import = format!("/companies/{company_id}/bank-accounts/{account}/statement-import");
    assert_eq!(send(&app, statement_request(&import, Some(&cookie), STATEMENT)).await.status, StatusCode::OK);

    // An aggregator that is down is a 502 that says nothing of what it answered, and the connection says why.
    aggregator.set_down(true);
    let down = send(&app, json_request("POST", &format!("{uri}/{id}/sync"), Some(&cookie), json!({}))).await;
    assert_eq!(down.status, StatusCode::BAD_GATEWAY);
    assert!(!down.body.to_string().contains("fake aggregator"));
    assert!(send(&app, get_request(&uri, Some(&cookie))).await.body["items"][0]["last_error"].as_str().is_some());
    aggregator.set_down(false);
    // The bank needing the person stops the pull and says so.
    aggregator.set_state("item-1", conta_verde_application::ItemState::NeedsAttention("the bank login failed".into()));
    let needs = send(&app, json_request("POST", &format!("{uri}/{id}/sync"), Some(&cookie), json!({}))).await;
    assert_eq!(needs.body["needs_attention"], true);
    let waiting = send(&app, get_request(&uri, Some(&cookie))).await;
    assert_eq!(
        (waiting.body["items"][0]["status"].as_str(), waiting.body["items"][0]["last_error"].as_str()),
        (Some("needs_attention"), Some("the bank login failed"))
    );
    // The trail says who connected it and what changed, and no secret exists to say.
    let history = send(&app, get_request(&format!("{uri}/{id}/history"), Some(&cookie))).await;
    assert!(history.body.as_array().unwrap().len() >= 2);
    assert_eq!(history.body[0]["actor"]["kind"], "user");

    // Ended, it frees the bank account and the aggregator is told; ending it again is the same.
    let ended = send(&app, json_request("POST", &format!("{uri}/{id}/disconnect"), Some(&cookie), json!({}))).await;
    assert_eq!(ended.body["status"], "disconnected");
    assert_eq!(aggregator.disconnected(), ["item-1"]);
    assert_eq!(send(&app, json_request("POST", &format!("{uri}/{id}/disconnect"), Some(&cookie), json!({}))).await.status, StatusCode::OK);
    assert_eq!(send(&app, json_request("POST", &format!("{uri}/{id}/sync"), Some(&cookie), json!({}))).await.status, StatusCode::UNPROCESSABLE_ENTITY);

    // Another company's connection is nobody's.
    let (stranger, stranger_company) = sign_up(&app, "bia@example.com").await;
    let theirs = format!("/companies/{stranger_company}/bank-connections/{id}");
    for suffix in ["sync", "disconnect"] {
        assert_eq!(
            send(&app, json_request("POST", &format!("{theirs}/{suffix}"), Some(&stranger), json!({}))).await.status,
            StatusCode::NOT_FOUND,
            "{suffix}"
        );
    }
    assert_eq!(send(&app, get_request(&format!("{theirs}/history"), Some(&stranger))).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bank_connections_follow_the_finance_role() {
    let (app, identities) = entry_app_with_identities();
    let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
    let account = registered_account(&app, &cookie, company_id, "Caixa").await;
    let uri = format!("/companies/{company_id}/bank-connections");
    this_aggregator().with_account("item-1", "acc-1", "Conta");
    let body = json!({ "bank_account_id": account, "item_id": "item-1", "account_id": "acc-1" });
    // Who looks at finance reads the connections; who writes it connects, pulls and ends them.
    for (role, writes) in [
        (conta_verde_domain::Role::Viewer, false),
        (conta_verde_domain::Role::Sales, false),
        (conta_verde_domain::Role::Stock, false),
        (conta_verde_domain::Role::Accountant, false),
        (conta_verde_domain::Role::Financial, true),
    ] {
        let member = member_with_role(&app, &identities, company_id, role).await;
        assert_eq!(send(&app, get_request(&uri, Some(&member))).await.status, StatusCode::OK, "{role:?}");
        let attempt = send(&app, json_request("POST", &uri, Some(&member), body.clone())).await;
        assert_eq!(attempt.status != StatusCode::FORBIDDEN, writes, "{role:?} {}", attempt.body);
        if writes {
            // Only the one connection can be made: the others were refused before they could be.
            assert_eq!(attempt.status, StatusCode::CREATED);
        }
    }
    assert_eq!(send(&app, get_request(&uri, None)).await.status, StatusCode::UNAUTHORIZED);
}
