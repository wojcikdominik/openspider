use os_fetch::resolve::HostResolver;
use os_testsite::mocks::{ai::AiMock, MockServer};
use std::{sync::Arc, time::Duration};

// spec: CRAWL-123 [M4]
#[tokio::test]
async fn ai_clients_use_the_crawler_wildcard_resolver_and_preserve_host() {
    let mock = AiMock::fixtures().unwrap();
    let server = MockServer::start(mock.registrations()).await.unwrap();
    let resolver =
        HostResolver::new(&["*.mock.test=127.0.0.1".into()], Duration::from_secs(2)).unwrap();
    let client = reqwest::Client::builder()
        .no_proxy()
        .dns_resolver(Arc::new(resolver))
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    for (host, path) in [
        ("openai.mock.test", "/openai/v1/models"),
        ("eu.openai.mock.test", "/openai/v1/models"),
        ("openrouter.mock.test", "/openrouter/api/v1/models"),
        ("eu.openrouter.mock.test", "/openrouter/api/v1/models"),
    ] {
        let response = client
            .get(format!("http://{host}:{}{path}", server.addr().port()))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            mock.requests().last().unwrap().host,
            format!("{host}:{}", server.addr().port())
        );
    }
    let no_override = reqwest::Client::builder()
        .no_proxy()
        .dns_resolver(Arc::new(
            HostResolver::new(&[], Duration::from_secs(2)).unwrap(),
        ))
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    assert!(no_override
        .get(format!(
            "http://openai.mock.test:{}/openai/v1/models",
            server.addr().port()
        ))
        .send()
        .await
        .is_err());
    assert_eq!(mock.requests().len(), 4);
    server.stop().await;
}
