use crate::error::{AppError, Result};
use std::net::{Ipv4Addr, SocketAddr};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
#[derive(Debug, PartialEq)]
pub struct Callback {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}
pub async fn bind(port: u16) -> Result<TcpListener> {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST,port))).await.map_err(|e|AppError::Auth(if e.kind()==std::io::ErrorKind::AddrInUse{format!("OAuth callback port {port} is already in use; stop the other process and retry")}else{format!("cannot bind OAuth callback: {e}")}))
}
pub async fn wait(listener: TcpListener, expected: &str) -> Result<String> {
    let result = tokio::time::timeout(std::time::Duration::from_secs(300), async {
        loop {
            let (mut stream, _) = listener.accept().await?;
            let mut buf = vec![0; 16384];
            let n = stream.read(&mut buf).await?;
            let parsed = parse(&String::from_utf8_lossy(&buf[..n]));
            if let Some(err) = parsed.error {
                respond(&mut stream, "400 Bad Request", "Authentication failed").await;
                return Err(AppError::Auth(format!("OAuth callback failed: {err}")));
            }
            if let Some(code) = parsed.code {
                let ok = parsed
                    .state
                    .as_deref()
                    .is_some_and(|s| s.as_bytes().ct_eq(expected.as_bytes()).into());
                if ok {
                    respond(
                        &mut stream,
                        "200 OK",
                        "Authentication successful. You may close this tab.",
                    )
                    .await;
                    return Ok(code);
                }
                respond(
                    &mut stream,
                    "400 Bad Request",
                    "State mismatch; return to the terminal and retry.",
                )
                .await;
                return Err(AppError::Auth("OAuth state mismatch".into()));
            }
            respond(&mut stream, "404 Not Found", "").await;
        }
    })
    .await
    .map_err(|_| AppError::Auth("OAuth callback timed out after 5 minutes".into()))?;
    result
}
async fn respond(s: &mut tokio::net::TcpStream, status: &str, msg: &str) {
    let body = format!("<html><body>{msg}</body></html>");
    let r=format!("HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
    let _ = s.write_all(r.as_bytes()).await;
}
pub fn parse(request: &str) -> Callback {
    let mut out = Callback {
        code: None,
        state: None,
        error: None,
    };
    let path = request
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap_or("");
    if let Some(q) = path.split_once('?').map(|v| v.1) {
        for (k, v) in url::form_urlencoded::parse(q.as_bytes()) {
            match k.as_ref() {
                "code" => out.code = Some(v.into_owned()),
                "state" => out.state = Some(v.into_owned()),
                "error" => out.error = Some(v.into_owned()),
                _ => {}
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses() {
        assert_eq!(
            parse("GET /auth/callback?code=a%2Bb&state=x HTTP/1.1\r\n\r\n"),
            Callback {
                code: Some("a+b".into()),
                state: Some("x".into()),
                error: None
            }
        );
    }
    #[test]
    fn error() {
        assert_eq!(
            parse("GET /auth/callback?error=denied HTTP/1.1\r\n\r\n")
                .error
                .as_deref(),
            Some("denied")
        );
    }
}
