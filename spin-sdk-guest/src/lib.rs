use anyhow::Result;
use spin_sdk::{
    http::{Request, Response},
    http_component,
};

#[http_component]
fn handle_foo(req: Request) -> Result<Response> {
    assert_eq!("/foo?a=b", &req.uri().to_string());
    assert_eq!(
        &[("what", "up")] as &[_],
        &req.headers()
            .iter()
            .map(|(k, v)| Ok((k.as_str(), v.to_str()?)))
            .collect::<Result<Vec<_>>>()?
    );
    assert_eq!(Some(b"hello, world!" as &[_]), req.into_body().as_deref());

    Ok(http::Response::builder()
        .header("content-type", "text/plain")
        .body(Some("hola, mundo!".into()))?)
}
