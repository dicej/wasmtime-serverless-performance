use http_types::{Request, Response};

wit_bindgen::generate!({
    world: "spin-http",
    path: "../wit"
});

struct InboundHttp;

impl inbound_http::InboundHttp for InboundHttp {
    fn handle_request(req: Request) -> Response {
        assert_eq!("/foo?a=b", &req.uri);
        assert_eq!(
            &[("what".to_owned(), "up".to_owned())] as &[_],
            &req.headers
        );
        assert_eq!(Some(b"hello, world!" as &[_]), req.body.as_deref());

        Response {
            status: 200,
            headers: Some(vec![("content-type".to_owned(), "text/plain".to_owned())]),
            body: Some(b"hola, mundo!".to_vec()),
        }
    }
}

export_spin_http!(InboundHttp);

#[export_name = "canonical_abi_free"]
unsafe fn canonical_abi_free(_ptr: *mut u8, _size: usize, _align: usize) {
    unreachable!()
}
