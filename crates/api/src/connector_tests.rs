// Included in the `tests` module of `lib.rs` after the bank connection tests: the e-commerce connectors ([ADR 0090]).

const CONNECTOR_ADDRESS: &str = "https://woo-bridge.example.com/webhooks/conta-verde";

#[tokio::test]
async fn a_connector_is_turned_on_off_and_on_again_through_the_keys_and_subscriptions_of_the_company() {
    let (app, _) = entry_app_with_identities();
    let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
    let uri = format!("/companies/{company_id}/connectors");

    // The catalogue says what each kind asks for; nothing is made until a connector is turned on.
    let first = send(&app, get_request(&uri, Some(&cookie))).await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    assert_eq!(first.body["available"], true);
    assert_eq!(first.body["catalogue"][0]["kind"], "woocommerce");
    assert_eq!(first.body["catalogue"][0]["scopes"], json!(["products:read", "sales:write"]));
    assert_eq!(first.body["catalogue"][0]["events"], json!(["sale.completed", "sale.cancelled"]));
    assert_eq!(first.body["items"].as_array().unwrap().len(), 0);

    // Refused: a kind the catalogue does not have, an empty name, a name used.
    for body in [json!({ "kind": "shopify", "name": "Loja" }), json!({ "kind": "woocommerce", "name": "  " })] {
        let refused = send(&app, json_request("POST", &uri, Some(&cookie), body.clone())).await;
        assert!(refused.status.is_client_error(), "{body}: {}", refused.body);
    }
    let made = send(&app, json_request("POST", &uri, Some(&cookie), json!({ "kind": "woocommerce", "name": "Loja do centro" }))).await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.body);
    assert_eq!((made.body["state"].as_str(), made.body["active"].as_bool()), (Some("off"), Some(false)));
    let id = made.body["id"].as_str().unwrap().to_owned();
    assert_eq!(
        send(&app, json_request("POST", &uri, Some(&cookie), json!({ "kind": "woocommerce", "name": "loja DO centro" }))).await.status,
        StatusCode::CONFLICT
    );

    // An address the subscription refuses makes nothing: no key is left usable.
    let bad = send(&app, json_request("POST", &format!("{uri}/{id}/activate"), Some(&cookie), json!({ "address": "not an address" }))).await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", bad.body);
    let keys = send(&app, get_request(&format!("/companies/{company_id}/api-keys"), Some(&cookie))).await;
    assert!(keys.body["items"].as_array().unwrap().iter().all(|key| key["status"] == "revoked"), "{}", keys.body);

    // Turned on, it gives the two secrets once; the key opens what the package needs and nothing more.
    let on = send(&app, json_request("POST", &format!("{uri}/{id}/activate"), Some(&cookie), json!({ "address": CONNECTOR_ADDRESS }))).await;
    assert_eq!(on.status, StatusCode::OK, "{}", on.body);
    let secret = on.body["api_key_secret"].as_str().unwrap().to_owned();
    let hook_secret = on.body["webhook_secret"].as_str().unwrap().to_owned();
    assert!(secret.starts_with("cvk_") && hook_secret.starts_with("whsec_"));
    assert_eq!(on.body["connector"]["state"], "on");
    assert_eq!(send(&app, bearer_request("GET", "/v1/products", &secret, None)).await.status, StatusCode::OK);
    assert_eq!(send(&app, bearer_request("GET", "/v1/parties", &secret, None)).await.status, StatusCode::FORBIDDEN);
    let keys = send(&app, get_request(&format!("/companies/{company_id}/api-keys"), Some(&cookie))).await;
    let key = keys.body["items"].as_array().unwrap().iter().find(|key| key["status"] == "active").unwrap();
    assert_eq!(key["name"], "Conector WooCommerce: Loja do centro");
    assert_eq!(key["scopes"], json!(["products:read", "sales:write"]));
    let subscriptions = send(&app, get_request(&format!("/companies/{company_id}/webhook-subscriptions"), Some(&cookie))).await;
    let subscription = &subscriptions.body["items"][0];
    assert_eq!((subscription["url"].as_str(), subscription["active"].as_bool()), (Some(CONNECTOR_ADDRESS), Some(true)));
    // Neither secret is read back from the list or the history.
    let listed = send(&app, get_request(&uri, Some(&cookie))).await;
    assert_eq!(listed.body["items"][0]["state"], "on");
    let history = send(&app, get_request(&format!("{uri}/{id}/history"), Some(&cookie))).await;
    assert_eq!(history.body.as_array().unwrap().len(), 2);
    let written = format!("{} {}", listed.body, history.body);
    assert!(!written.contains(&secret) && !written.contains(&hook_secret));
    // On twice is refused and makes nothing more.
    assert_eq!(
        send(&app, json_request("POST", &format!("{uri}/{id}/activate"), Some(&cookie), json!({ "address": CONNECTOR_ADDRESS }))).await.status,
        StatusCode::CONFLICT
    );

    // The person revoking the key elsewhere is seen, not hidden.
    let key_id = key["id"].as_str().unwrap().to_owned();
    send(&app, json_request("POST", &format!("/companies/{company_id}/api-keys/{key_id}/revoke"), Some(&cookie), json!({}))).await;
    let attention = send(&app, get_request(&uri, Some(&cookie))).await;
    assert_eq!(attention.body["items"][0]["state"], "attention");

    // Turned off it revokes the key and switches the subscription off, which stays for next time.
    let off = send(&app, json_request("POST", &format!("{uri}/{id}/deactivate"), Some(&cookie), json!({}))).await;
    assert_eq!((off.status, off.body["state"].as_str()), (StatusCode::OK, Some("off")), "{}", off.body);
    assert_eq!(send(&app, bearer_request("GET", "/v1/products", &secret, None)).await.status, StatusCode::UNAUTHORIZED);
    let subscriptions = send(&app, get_request(&format!("/companies/{company_id}/webhook-subscriptions"), Some(&cookie))).await;
    assert_eq!(subscriptions.body["items"][0]["active"], false);

    // On again: a new key, the same subscription on, a new secret.
    let again = send(&app, json_request("POST", &format!("{uri}/{id}/activate"), Some(&cookie), json!({ "address": CONNECTOR_ADDRESS }))).await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.body);
    assert_ne!(again.body["api_key_secret"].as_str().unwrap(), secret);
    assert_ne!(again.body["webhook_secret"].as_str().unwrap(), hook_secret);
    assert_eq!(send(&app, bearer_request("GET", "/v1/products", again.body["api_key_secret"].as_str().unwrap(), None)).await.status, StatusCode::OK);
    let subscriptions = send(&app, get_request(&format!("/companies/{company_id}/webhook-subscriptions"), Some(&cookie))).await;
    assert_eq!(subscriptions.body["items"].as_array().unwrap().len(), 1);
    assert_eq!(subscriptions.body["items"][0]["active"], true);

    // Another company's connector is nobody's.
    let (stranger, stranger_company) = sign_up(&app, "bia@example.com").await;
    let theirs = format!("/companies/{stranger_company}/connectors/{id}");
    for suffix in ["activate", "deactivate"] {
        assert_eq!(
            send(&app, json_request("POST", &format!("{theirs}/{suffix}"), Some(&stranger), json!({ "address": CONNECTOR_ADDRESS }))).await.status,
            StatusCode::NOT_FOUND,
            "{suffix}"
        );
    }
    assert_eq!(send(&app, get_request(&format!("{theirs}/history"), Some(&stranger))).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn connectors_belong_to_the_owners_and_admins_alone() {
    let (app, identities) = entry_app_with_identities();
    let (cookie, company_id) = sign_up(&app, "ana@example.com").await;
    let uri = format!("/companies/{company_id}/connectors");
    for role in [conta_verde_domain::Role::Viewer, conta_verde_domain::Role::Financial, conta_verde_domain::Role::Sales, conta_verde_domain::Role::Accountant] {
        let member = member_with_role(&app, &identities, company_id, role).await;
        assert_eq!(send(&app, get_request(&uri, Some(&member))).await.status, StatusCode::FORBIDDEN, "{role:?} reads");
        assert_eq!(
            send(&app, json_request("POST", &uri, Some(&member), json!({ "kind": "woocommerce", "name": "Loja" }))).await.status,
            StatusCode::FORBIDDEN,
            "{role:?} writes"
        );
    }
    assert_eq!(send(&app, get_request(&uri, None)).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(send(&app, get_request(&uri, Some(&cookie))).await.status, StatusCode::OK);
}
