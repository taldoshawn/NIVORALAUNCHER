//! Regression probe through the public client API. It avoids the unrelated
//! internal libtest modules and never opens a game or touches player data.
use horizon_client::net::{Event, Session};
use horizon_protocol::{
    connection::Connection,
    packets::{configuration as cfg, handshake},
    text::Language,
};
use std::{path::PathBuf, sync::Arc, time::Duration};

fn check_login_rejection() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let session = Session::connect_legacy(
        &format!("localhost:{port}"),
        "Probe",
        None,
        cfg::ClientInformation::new(8),
        Arc::new(Language::new()),
        PathBuf::new(),
    );
    let (stream, _) = listener.accept().unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut server = Connection::from_stream(stream).unwrap();
    let (id, body) = server.recv().unwrap();
    let handshake::Serverbound::Intention(packet) =
        handshake::Serverbound::decode(id, &body).unwrap()
    else {
        panic!("expected legacy login handshake");
    };
    assert_eq!(packet.protocol, 47);
    assert_eq!(server.recv().unwrap().0, 0);
    let mut kick = horizon_protocol::buf::Writer::new();
    kick.varint(0);
    kick.string(r#"{"text":"MUSH\nValide sua conexão. ","extra":[{"text":"Clique aqui.","clickEvent":{"action":"open_url","value":"https://mush.com.br/validar"}}]}"#);
    server.send(&kick.data).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        assert!(
            std::time::Instant::now() < deadline,
            "missing disconnect event"
        );
        for event in session.poll() {
            match event {
                Event::Disconnected {
                    reason,
                    key,
                    from_server,
                } => {
                    assert_eq!(
                        reason,
                        "MUSH\nValide sua conexão. Clique aqui.\nhttps://mush.com.br/validar"
                    );
                    assert!(from_server);
                    assert!(!horizon_client::reconnect::allowed(
                        &reason,
                        key.as_deref(),
                        from_server
                    ));
                    return;
                }
                Event::Status(_) => {}
                _ => panic!("a rejected login must not join play"),
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn check_disconnect_layout() {
    let screen = horizon_client::multiplayer::Screen::Disconnected {
        reason: "Valide sua conexão\nhttps://mush.com.br/validar".into(),
        retry: None,
    };
    let buttons = screen.buttons(427.0, 253.0);
    assert_eq!((buttons[0].0, buttons[0].1), (113.0, 201.0));
}

fn main() {
    check_login_rejection();
    check_disconnect_layout();
    println!("Legacy login component, validation URL, reconnect and disconnect layout: passed");
}

#[test]
fn login_rejection_is_readable_and_does_not_trigger_reconnect() {
    check_login_rejection();
}

#[test]
fn disconnect_navigation_leaves_room_for_server_instructions() {
    check_disconnect_layout();
}
