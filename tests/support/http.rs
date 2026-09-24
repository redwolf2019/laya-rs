//! Minimal HTTP/1 test client; every request asks the server to close the connection.
use std::net::SocketAddr;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{Duration, timeout},
};

pub async fn send(address: SocketAddr, wire: &[u8]) -> TcpStream {
    let mut socket = TcpStream::connect(address).await.unwrap();
    socket.write_all(wire).await.unwrap();
    socket
}

pub async fn post(address: SocketAddr, body: &str) -> TcpStream {
    send(address, format!(
        "POST /v1/system-one HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()
    ).as_bytes()).await
}

pub async fn receive(mut socket: TcpStream) -> (u16, String, Vec<u8>) {
    let mut response = Vec::new();
    timeout(Duration::from_secs(30), socket.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    let split = response.windows(4).position(|s| s == b"\r\n\r\n").unwrap();
    let headers = std::str::from_utf8(&response[..split]).unwrap().to_owned();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, headers, response[split + 4..].to_vec())
}

pub async fn get(address: SocketAddr, path: &str) -> (u16, String, Vec<u8>) {
    receive(
        send(
            address,
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await,
    )
    .await
}
