//! Native 1.7 transport, sharing the renderer's normalized legacy event model.
use super::*;
use horizon_protocol::{
    legacy17::{self, Version, bridge::Incoming, play::Packet},
    legacy18::bridge::Outbound,
};

pub(super) fn run(
    version: Version,
    local: Option<horizon_protocol::connection::Pipe>,
    address: &str,
    name: &str,
    auth: Option<&AuthSession>,
    lang: &Language,
    send: &Sender<Event>,
    writer: &Mutex<Option<ConnWriter>>,
    configuring: &AtomicBool,
    counters: &Mutex<Option<Arc<Counters>>>,
    bridge: &Mutex<Outbound>,
) -> Result<Reason, Reason> {
    status(send, &tr(lang, "connect.connecting"));
    let (connection, host, port) = match local {
        Some(pipe) => (Connection::local(pipe), "localhost".to_owned(), 25565),
        None => {
            let (host, port) =
                parse_address(address).ok_or_else(|| Reason::plain("Invalid server address"))?;
            connect_tcp(&host, port, false, READ_TIMEOUT).map_err(Reason::plain)?
        }
    };
    *counters.lock().unwrap() = Some(connection.reader.counters.clone());
    status(send, "Logging in to Minecraft 1.7...");
    let outcome = legacy17::login_outcome(
        version,
        connection,
        &host,
        port,
        name,
        READ_TIMEOUT,
        |hash| {
            let account = auth.ok_or_else(|| {
                horizon_protocol::Error::new("This server requires a signed-in Minecraft account")
            })?;
            crate::auth::join_server(account, hash).map_err(|e| {
                horizon_protocol::Error::new(match e.reason() {
                    Ok(key) => tr(lang, key),
                    Err(message) => message.to_owned(),
                })
            })
        },
    )
    .map_err(|error| Reason::plain(connection_lost(error, lang)))?;
    let (connection, profile) = match outcome {
        legacy17::LoginOutcome::Joined(connection, profile) => (connection, profile),
        legacy17::LoginOutcome::Disconnected(reason) => {
            return Ok(Reason::component(&reason, lang));
        }
    };
    status(send, "Joining Minecraft 1.7 world...");
    let (mut reader, outgoing) = connection.split();
    *writer.lock().unwrap() = Some(outgoing);
    configuring.store(false, Ordering::Relaxed);
    let _ = send.send(Event::Joined {
        registries: Arc::new(legacy18::registries()),
        uuid: profile.uuid,
        name: profile.name,
    });
    let mut incoming = Incoming::new(version);
    loop {
        let (id, payload) = reader
            .recv()
            .map_err(|error| Reason::plain(connection_lost(error, lang)))?;
        match Packet::decode(id, &payload).map_err(|error| Reason::plain(error.to_string()))? {
            Packet::KeepAlive(value) => {
                if let Some(w) = writer.lock().unwrap().as_mut() {
                    w.send(&legacy17::play::keep_alive(value))
                        .map_err(|error| Reason::plain(error.to_string()))?;
                }
                continue;
            }
            Packet::Disconnect(reason) => return Ok(Reason::component(&reason, lang)),
            Packet::Join { entity, .. } => bridge.lock().unwrap().entity = entity,
            _ => {}
        }
        for (id, payload) in incoming
            .translate(id, &payload)
            .map_err(|error| Reason::plain(error.to_string()))?
        {
            // Release the action-model lock before acquiring the socket lock,
            // matching Session::send's lock ordering and preventing deadlock.
            let response = {
                let mut model = bridge.lock().unwrap();
                let response = model
                    .observe(id, &payload)
                    .map_err(|error| Reason::plain(error.to_string()))?;
                response
                    .map(|bytes| legacy17::bridge::outgoing(&bytes, model.entity))
                    .transpose()
                    .map_err(|error| Reason::plain(error.to_string()))?
            };
            if let Some(bytes) = response
                && let Some(w) = writer.lock().unwrap().as_mut()
            {
                w.send(&bytes)
                    .map_err(|error| Reason::plain(error.to_string()))?;
            }
            if send.send(Event::LegacyPacket { id, payload }).is_err() {
                return Ok(Reason::plain("Game disconnected"));
            }
        }
    }
}
