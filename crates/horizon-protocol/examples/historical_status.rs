//! Query the advertised version and round-trip time of an existing server.
use horizon_protocol::{
    connection::Connection,
    packets::{Empty, handshake, status},
};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [version, host, port] = args.as_slice() else {
        return Err("usage: historical_status <version|protocol-number|auto> <host> <port>".into());
    };
    let protocol = match version.as_str() {
        "auto" => None,
        "1.7.2" | "1.7.3" | "1.7.4" | "1.7.5" => Some(4),
        "1.7.6" | "1.7.7" | "1.7.8" | "1.7.9" | "1.7.10" => Some(5),
        "1.8" | "1.8.1" | "1.8.2" | "1.8.3" | "1.8.4" | "1.8.5" | "1.8.6" | "1.8.7" | "1.8.8"
        | "1.8.9" => Some(47),
        "1.12.2" => Some(340),
        "1.20.1" => Some(763),
        horizon_core::VANILLA_VERSION => Some(horizon_protocol::PROTOCOL_VERSION),
        number => Some(
            number
                .parse::<i32>()
                .ok()
                .filter(|&number| number >= 0)
                .ok_or("use auto or the server's numeric protocol for this version")?,
        ),
    };
    if host.is_empty() || host.encode_utf16().count() > 255 || host.contains('\0') {
        return Err("invalid server host".into());
    }
    let port: u16 = port.parse()?;
    let timeout = Duration::from_secs(5);
    let mut connection = Connection::connect_server(host, port, timeout)?;
    connection.reader.set_read_timeout(Some(timeout))?;
    connection.send(
        &handshake::Serverbound::Intention(handshake::Intention {
            protocol: protocol.unwrap_or(-1),
            host: host.clone(),
            port,
            intent: handshake::STATUS,
        })
        .encode(),
    )?;
    connection.send(&status::Serverbound::StatusRequest(Empty).encode())?;
    let (id, payload) = connection.recv()?;
    let status::Clientbound::StatusResponse(reply) = status::Clientbound::decode(id, &payload)?
    else {
        return Err("expected a status response".into());
    };
    let json: serde_json::Value = serde_json::from_str(&reply.json)?;
    let advertised = json["version"]["protocol"]
        .as_i64()
        .ok_or("server did not advertise a protocol number")?;
    let started = Instant::now();
    let token = 0x4e49_564f_5241;
    connection.send(&status::Serverbound::PingRequest(status::Ping { time: token }).encode())?;
    let (id, payload) = connection.recv()?;
    match status::Clientbound::decode(id, &payload)? {
        status::Clientbound::PongResponse(pong) if pong.time == token => {}
        _ => return Err("server returned an invalid pong".into()),
    }
    println!(
        "{}",
        serde_json::json!({
            "requested_protocol": protocol,
            "advertised_protocol": advertised,
            "protocol_matches": protocol.map(|number| advertised == i64::from(number)),
            "ping_ms": started.elapsed().as_millis(),
            "status": json,
        })
    );
    Ok(())
}
