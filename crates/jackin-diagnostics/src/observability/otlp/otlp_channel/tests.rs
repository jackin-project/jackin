use super::*;

fn key(endpoint: &str) -> ChannelKey {
    ChannelKey::new(endpoint, Duration::from_secs(1), &TlsConfig::default())
}

#[tokio::test]
async fn channels_are_shared_only_for_identical_transport_configuration() {
    let shared = PhysicalChannels::build([
        key("http://127.0.0.1:4317"),
        key("http://127.0.0.1:4317"),
        key("http://127.0.0.1:4317"),
    ])
    .unwrap();
    assert_eq!(shared.len(), 1);

    let distinct =
        PhysicalChannels::build([key("http://127.0.0.1:4317"), key("http://127.0.0.1:4318")])
            .unwrap();
    assert_eq!(distinct.len(), 2);
}

#[test]
fn failed_attempt_buffer_is_bounded_and_drains_only_after_recovery() {
    let connector = PhysicalConnector::new(&key("http://127.0.0.1:4317")).unwrap();
    let failure = AttemptObservation {
        elapsed: Duration::from_millis(1),
        outcome: OutcomeValue::Failure,
        error: Some(ErrorType::ConnectionRefused),
    };
    for _ in 0..(MAX_PENDING_ATTEMPTS + 20) {
        connector.record(failure);
    }
    assert_eq!(connector.pending_len(), MAX_PENDING_ATTEMPTS);

    connector.record(AttemptObservation {
        elapsed: Duration::from_millis(1),
        outcome: OutcomeValue::Success,
        error: None,
    });
    assert_eq!(connector.pending_len(), 0);
}

#[tokio::test]
async fn plaintext_connector_owns_the_physical_connection() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let accepted = tokio::spawn(async move { listener.accept().await.unwrap() });
    let uri = format!("http://{address}").parse().unwrap();
    let io = connect_physical(uri, None).await.unwrap();
    assert!(matches!(io, PhysicalIo::Plain(_)));
    drop(io);
    accepted.await.unwrap();
}

#[tokio::test]
async fn recovered_plaintext_reconnect_drains_the_failed_attempt() {
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let endpoint = format!("http://{address}");
    let mut connector = PhysicalConnector::new(&key(&endpoint)).unwrap();
    let uri = endpoint.parse().unwrap();
    connector
        .call(uri)
        .await
        .expect_err("closed port must fail the physical attempt");
    assert_eq!(connector.pending_len(), 1);

    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let accepted = tokio::spawn(async move { listener.accept().await.unwrap() });
    let uri = endpoint.parse().unwrap();
    drop(connector.call(uri).await.unwrap());
    accepted.await.unwrap();
    assert_eq!(connector.pending_len(), 0);
}

#[tokio::test]
async fn connector_timeout_is_owned_as_a_physical_attempt() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "https://localhost:{}",
        listener.local_addr().unwrap().port()
    );
    let stalled_peer = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    let timeout_key = ChannelKey::new(&endpoint, Duration::from_millis(10), &TlsConfig::default());
    let mut connector = PhysicalConnector::new(&timeout_key).unwrap();
    let error = connector
        .call(endpoint.parse().unwrap())
        .await
        .expect_err("zero connect budget must time out");
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert_eq!(connector.pending_len(), 1);
    stalled_peer.abort();
}

#[tokio::test]
async fn tls_handshake_is_inside_the_physical_attempt() {
    let certified = rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap();
    let certificate_der = certified.cert.der().clone();
    let key_der =
        rustls::pki_types::PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
    let mut server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certificate_der], key_der.into())
        .unwrap();
    server_config.alpn_protocols = vec![b"h2".to_vec()];
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));
    let accepted = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        acceptor.accept(stream).await
    });

    let directory = tempfile::tempdir().unwrap();
    let ca = directory.path().join("ca.pem");
    std::fs::write(&ca, certified.cert.pem()).unwrap();
    let tls = TlsConfig {
        certificate: Some(ca.to_string_lossy().into_owned()),
        client_key: None,
        client_certificate: None,
    };
    let config = tls_config(&ChannelKey::new(
        &format!("https://localhost:{}", address.port()),
        Duration::from_secs(1),
        &tls,
    ))
    .unwrap();
    let uri = format!("https://localhost:{}", address.port())
        .parse()
        .unwrap();
    let io = connect_physical(uri, Some(Arc::new(config))).await.unwrap();
    assert!(matches!(io, PhysicalIo::Tls(_)));
    drop(io);
    accepted.await.unwrap().unwrap();
}

#[tokio::test]
async fn untrusted_tls_certificate_fails_the_physical_attempt() {
    let certified = rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap();
    let key_der =
        rustls::pki_types::PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![certified.cert.der().clone()], key_der.into())
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));
    let accepted = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        drop(acceptor.accept(stream).await);
    });
    let config = tls_config(&key(&format!("https://localhost:{}", address.port()))).unwrap();
    let uri = format!("https://localhost:{}", address.port())
        .parse()
        .unwrap();
    let error = connect_physical(uri, Some(Arc::new(config)))
        .await
        .expect_err("untrusted server certificate must fail the TLS attempt");
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    accepted.await.unwrap();
}
