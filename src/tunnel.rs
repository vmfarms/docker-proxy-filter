use tracing::*;
use tokio::io::copy_bidirectional;
use tokio::net::TcpStream;

/// Returns true for Docker API endpoints that require HTTP connection hijacking.
/// These endpoints upgrade from HTTP to raw bidirectional TCP streams and cannot
/// be proxied through a normal HTTP reverse proxy.
pub fn is_upgrade_path(path: &str) -> bool {
    // POST /exec/{id}/start -- raw multiplexed stream for exec I/O
    if path.contains("/exec/") && path.contains("/start") {
        return true;
    }
    // POST /containers/{id}/attach (but not /attach/ws which is websocket)
    if path.contains("/attach") && !path.contains("/attach/ws") {
        return true;
    }
    false
}

/// Peek at the first line of an HTTP request to extract method + path.
/// Returns (method, path, header_end_pos) or None if not enough data.
pub fn peek_request_line(buf: &[u8]) -> Option<(String, String)> {
    // Find the first \r\n
    let line_end = buf.windows(2).position(|w| w == b"\r\n")?;
    let line = std::str::from_utf8(&buf[..line_end]).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    Some((method, path))
}

/// Handle a single client connection. Peeks at the HTTP request to determine
/// if it's an upgrade endpoint. If so, tunnels raw TCP. Otherwise, forwards
/// the connection to the ntex HTTP handler on the internal port.
pub async fn handle_connection(
    mut client: TcpStream,
    upstream_addr: String,
    http_addr: String,
) {
    // Peek at the request without consuming it
    let mut peek_buf = vec![0u8; 4096];
    let n = match client.peek(&mut peek_buf).await {
        Ok(n) if n > 0 => n,
        _ => {
            debug!("Client disconnected before sending data");
            return;
        }
    };

    let is_upgrade = match peek_request_line(&peek_buf[..n]) {
        Some((method, path)) => {
            let upgrade = is_upgrade_path(&path);
            if upgrade {
                info!("Upgrade endpoint detected: {} {} -- tunneling raw TCP", method, path);
            }
            upgrade
        }
        None => false,
    };

    // Pick destination: upgrade endpoints go direct to socket-proxy,
    // everything else goes to the internal ntex HTTP handler.
    let dest_addr = if is_upgrade { &upstream_addr } else { &http_addr };

    let mut upstream = match TcpStream::connect(dest_addr).await {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to connect to {}: {}", dest_addr, e);
            return;
        }
    };

    // Bidirectional copy -- just splice the two streams
    match copy_bidirectional(&mut client, &mut upstream).await {
        Ok((c2u, u2c)) => {
            debug!("Tunnel closed: {} bytes client->upstream, {} bytes upstream->client", c2u, u2c);
        }
        Err(e) => {
            // Connection reset is normal for Docker exec (client closes when done)
            let kind = std::io::Error::from(e).kind();
            if kind != std::io::ErrorKind::ConnectionReset {
                debug!("Tunnel error (kind={:?})", kind);
            }
        }
    }
}
