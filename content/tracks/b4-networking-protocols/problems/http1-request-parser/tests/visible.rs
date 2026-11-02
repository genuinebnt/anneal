use solution::*;

fn lim() -> Limits {
    Limits { max_head: 8192, max_headers: 32, max_body: 1 << 20 }
}

fn one(bytes: &[u8]) -> Result<Option<Request>, ParseError> {
    let mut p = Parser::new(lim());
    p.feed(bytes);
    p.next_request()
}

fn all(chunks: &[&[u8]], limits: Limits) -> Result<Vec<Request>, ParseError> {
    let mut p = Parser::new(limits);
    let mut out = Vec::new();
    for c in chunks {
        p.feed(c);
        while let Some(r) = p.next_request()? {
            out.push(r);
        }
    }
    Ok(out)
}

        #[test]
        fn a_simple_get() {
            let r = one(b"GET /index.html?x=1 HTTP/1.1\r\nHost: example.com\r\nAccept: */*\r\n\r\n").unwrap().unwrap();
            check!("method, target, version", (r.method.as_str(), r.target.as_str(), r.minor), ("GET", "/index.html?x=1", 1));
            check!("headers in order", r.headers, vec![("Host".to_string(), "example.com".to_string()), ("Accept".to_string(), "*/*".to_string())]);
            check!("no body", r.body.len(), 0);
        }

        #[test]
        fn a_post_with_content_length() {
            let r = one(b"POST /submit HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello").unwrap().unwrap();
            check!("the body", r.body, b"hello".to_vec());
        }

        #[test]
        fn header_lookup_ignores_case_and_trims_values() {
            let r = one(b"GET / HTTP/1.0\r\nhOsT:   spaced.example \t\r\n\r\n").unwrap().unwrap();
            check!("case-insensitive, trimmed", (r.header("HOST"), r.header("host"), r.header("nope"), r.minor), (Some("spaced.example"), Some("spaced.example"), None, 0));
        }

        #[test]
        fn a_chunked_body() {
            let r = one(b"POST /up HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6;ext=1\r\n world\r\n0\r\nTrailer: x\r\n\r\n").unwrap().unwrap();
            check!("chunks joined", r.body, b"hello world".to_vec());
        }

        #[test]
        fn incomplete_input_waits_and_pipelined_requests_come_out_in_order() {
            let mut p = Parser::new(lim());
            p.feed(b"GET /a HTTP/1.1\r\nHost: x\r\n");
            check!("no blank line yet", p.next_request(), Ok(None));
            p.feed(b"\r\nGET /b HTTP/1.1\r\n\r\nGET /c HT");
            let a = p.next_request().unwrap().unwrap();
            let b = p.next_request().unwrap().unwrap();
            check!("two complete, the third partial", (a.target.as_str(), b.target.as_str(), p.next_request()), ("/a", "/b", Ok(None)));
        }
