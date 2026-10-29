use reqwest::{Client, Method, Url};
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

pub fn server_origin(input: &str) -> Result<String, String> {
    let u = Url::parse(input.trim()).map_err(|_| "Enter a valid server address.")?;
    let local = matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if !(u.scheme() == "https" || (u.scheme() == "http" && local))
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || u.path() != "/"
    {
        return Err("Use an HTTPS server origin, without a path or credentials. HTTP is allowed only on loopback for local development.".into());
    }
    Ok(u.origin().ascii_serialization())
}
#[cfg(test)]
pub fn client() -> Result<Client, String> {
    client_with_session(None, None)
}
pub fn client_with_session(origin: Option<&str>, token: Option<&str>) -> Result<Client, String> {
    let jar = std::sync::Arc::new(reqwest::cookie::Jar::default());
    if let (Some(origin), Some(token)) = (origin, token) {
        let url = Url::parse(origin).map_err(|_| "Invalid remembered server.")?;
        jar.add_cookie_str(
            &format!(
                "market_session={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=31536000{}",
                if url.scheme() == "https" {
                    "; Secure"
                } else {
                    ""
                }
            ),
            &url,
        );
    }
    Client::builder()
        .cookie_provider(jar)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|_| "Cannot create the server connection.".into())
}
pub fn api_url(origin: &str, path: &str) -> Result<Url, String> {
    if path.len() > 4096
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['#', '\\'])
        || path.chars().any(char::is_control)
    {
        return Err("Invalid API path.".into());
    }
    let decoded = percent_encoding::percent_decode_str(path.split('?').next().unwrap_or(""))
        .decode_utf8()
        .map_err(|_| "Invalid API path.")?;
    if decoded.contains(['\\', '%']) || decoded.split('/').any(|s| s == "." || s == "..") {
        return Err("Invalid API path.".into());
    }
    let url = Url::parse(&format!("{origin}/v1{path}")).map_err(|_| "Invalid API path.")?;
    if url.origin().ascii_serialization() != origin || !url.path().starts_with("/v1/") {
        return Err("Invalid API destination.".into());
    }
    Ok(url)
}
#[derive(Serialize)]
pub struct Reply {
    pub status: u16,
    pub body: Value,
    #[serde(skip)]
    pub session_token: Option<String>,
}
pub async fn request(
    client: Client,
    origin: String,
    path: String,
    method: String,
    body: Option<Value>,
    csrf: Option<String>,
) -> Result<Reply, String> {
    let method = match method.as_str() {
        "GET" => Method::GET,
        "POST" => Method::POST,
        "PATCH" => Method::PATCH,
        "DELETE" => Method::DELETE,
        _ => return Err("Unsupported request method.".into()),
    };
    if body
        .as_ref()
        .is_some_and(|b| b.to_string().len() > 64 * 1024)
    {
        return Err("Request is too large.".into());
    }
    let mut req = client
        .request(method, api_url(&origin, &path)?)
        .header("Origin", &origin);
    if let Some(csrf) = csrf {
        req = req.header("X-CSRF-Token", csrf);
    }
    if let Some(body) = body {
        req = req.json(&body);
    }
    let mut response = req
        .send()
        .await
        .map_err(|_| "Cannot reach the server. Check the address and connection.")?;
    let status = response.status().as_u16();
    let session_token = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|h| h.to_str().ok())
        .find_map(|h| {
            h.split(';')
                .next()?
                .strip_prefix("market_session=")
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        });
    let mut bytes = vec![];
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "The server response was interrupted.")?
    {
        if bytes.len() + chunk.len() > 32 * 1024 * 1024 {
            return Err("Server response is too large. Narrow your search.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .map_err(|_| "The server did not return a valid API response.")?
    };
    Ok(Reply {
        status,
        body,
        session_token,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn origins_are_explicit_and_remote_http_is_refused() {
        assert_eq!(
            server_origin("https://example.com/").unwrap(),
            "https://example.com"
        );
        assert!(server_origin("http://127.0.0.1:8080").is_ok());
        for bad in [
            "http://example.com",
            "https://user:pass@example.com",
            "https://example.com/path",
            "https://example.com?token=a",
            "file:///tmp/test",
        ] {
            assert!(server_origin(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn request_paths_cannot_escape_the_api_or_change_host() {
        for bad in [
            "//other.com",
            "/../health",
            "/%2e%2e/health",
            "/%252e%252e/health",
            "/x#fragment",
            "/x\\y",
        ] {
            assert!(api_url("https://example.com", bad).is_err(), "{bad}");
        }
        assert_eq!(
            api_url("https://example.com", "/items?q=herb&limit=10")
                .unwrap()
                .as_str(),
            "https://example.com/v1/items?q=herb&limit=10"
        );
    }
    #[test]
    fn native_session_keeps_cookies_and_origin_and_does_not_follow_redirects() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let expected = origin.clone();
        let server = std::thread::spawn(move || {
            for step in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = vec![];
                loop {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                    if request.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
                assert!(request.contains(&format!("origin: {expected}")));
                if step > 0 {
                    assert!(request.contains("cookie: market_session=test-session"));
                }
                if step == 1 {
                    assert!(request.contains("x-csrf-token: test-csrf"));
                }
                let extra = if step == 0 {
                    "Set-Cookie: market_session=test-session; Path=/; HttpOnly; SameSite=Strict\r\n"
                } else if step == 2 {
                    "Location: https://example.invalid/\r\n"
                } else {
                    ""
                };
                let status = if step == 2 { "302 Found" } else { "200 OK" };
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n{extra}\r\n{{}}").unwrap();
            }
        });
        tauri::async_runtime::block_on(async {
            let c = client().unwrap();
            assert_eq!(
                request(
                    c.clone(),
                    origin.clone(),
                    "/auth/login".into(),
                    "POST".into(),
                    None,
                    None
                )
                .await
                .unwrap()
                .status,
                200
            );
            assert_eq!(
                request(
                    c.clone(),
                    origin.clone(),
                    "/watchlists".into(),
                    "POST".into(),
                    None,
                    Some("test-csrf".into())
                )
                .await
                .unwrap()
                .status,
                200
            );
            assert_eq!(
                request(c, origin, "/me".into(), "GET".into(), None, None)
                    .await
                    .unwrap()
                    .status,
                302
            );
        });
        server.join().unwrap();
    }
}
