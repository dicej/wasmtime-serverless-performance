from spin_http import Response

def handle_request(req):
    assert("/foo?a=b" == req.uri)
    assert([("what", "up")] == req.headers)
    assert(b"hello, world!" == req.body)

    return Response(200,
                    [("content-type", "text/plain")],
                    bytes("hola, mundo!", "utf-8"))
