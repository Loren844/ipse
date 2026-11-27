//! `ipse health`: whether `ipse serve` answers on this computer, for the Docker image's
//! health check. The image has no shell nor `curl`, only this binary.

use std::io::{Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

use ipse_core::{Error, Result};

/// How long the server has to answer.
const PATIENCE: Duration = Duration::from_secs(3);

/// Asks `ipse serve` on `port` of this computer for its sign-in page. Succeeds when it
/// answers, even with a refusal: a server that answers is up.
///
/// # Errors
///
/// [`Error::Refused`] when nothing answers in time, or the answer is not HTTP or is a
/// server error.
pub(crate) fn run(port: u16) -> Result<()> {
    let down = |why: &str| Error::Refused(format!("Ipse does not answer on port {port}: {why}."));
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream =
        TcpStream::connect_timeout(&address, PATIENCE).map_err(|e| down(&e.to_string()))?;
    stream
        .set_read_timeout(Some(PATIENCE))
        .and_then(|()| stream.set_write_timeout(Some(PATIENCE)))
        .map_err(|e| down(&e.to_string()))?;
    stream
        .write_all(
            format!("GET /login HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .map_err(|e| down(&e.to_string()))?;
    let mut head = [0_u8; 12];
    stream
        .read_exact(&mut head)
        .map_err(|e| down(&e.to_string()))?;
    let status = std::str::from_utf8(&head)
        .ok()
        .and_then(|h| h.strip_prefix("HTTP/1.1 "))
        .and_then(|code| code.parse::<u16>().ok());
    match status {
        Some(code) if code < 500 => Ok(()),
        Some(code) => Err(down(&format!("it answers {code}"))),
        None => Err(down("its answer is not HTTP")),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::io::{BufRead as _, BufReader};
    use std::net::TcpListener;

    use super::*;

    /// A server on this computer answering every request with `status`.
    fn answering(status: &'static str) -> u16 {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 && line != "\r\n" {
                    line.clear();
                }
                let _ = (&stream).write_all(
                    format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n").as_bytes(),
                );
            }
        });
        port
    }

    #[test]
    fn a_server_that_answers_is_up_even_when_it_refuses() {
        assert!(run(answering("200 OK")).is_ok());
        assert!(run(answering("403 Forbidden")).is_ok());
    }

    #[test]
    fn a_server_error_or_no_server_is_down() {
        assert!(run(answering("503 Service Unavailable")).is_err());
        let free = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);
        assert!(run(port).is_err());
    }
}
