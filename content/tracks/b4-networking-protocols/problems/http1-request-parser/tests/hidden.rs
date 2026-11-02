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
        fn bad_request_lines() {
            for bad in [&b"GET  / HTTP/1.1\r\n\r\n"[..], b"GET /\r\n\r\n", b"get / HTTP/1.1\r\n\r\n", b"GET / HTTP/1.1 extra\r\n\r\n", b"GET / FTP/1.1\r\n\r\n", b" / HTTP/1.1\r\n\r\n"] {
                check!("rejected request line", one(bad), Err(ParseError::BadRequestLine));
            }
        }

        #[test]
        fn bad_versions() {
            check!("HTTP/2.0 is not HTTP/1.x", one(b"GET / HTTP/2.0\r\n\r\n"), Err(ParseError::BadVersion));
            check!("HTTP/1.2 neither", one(b"GET / HTTP/1.2\r\n\r\n"), Err(ParseError::BadVersion));
        }

        #[test]
        fn header_syntax_is_strict() {
            check!("space before the colon (a smuggling vector)", one(b"GET / HTTP/1.1\r\nContent-Length : 5\r\n\r\n"), Err(ParseError::BadHeader));
            check!("obsolete line folding", one(b"GET / HTTP/1.1\r\nA: b\r\n c\r\n\r\n"), Err(ParseError::BadHeader));
            check!("no colon", one(b"GET / HTTP/1.1\r\nNoColonHere\r\n\r\n"), Err(ParseError::BadHeader));
            check!("empty name", one(b"GET / HTTP/1.1\r\n: v\r\n\r\n"), Err(ParseError::BadHeader));
        }

        #[test]
        fn both_framing_headers_are_refused() {
            check!("Content-Length and chunked together", one(b"POST / HTTP/1.1\r\nContent-Length: 4\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"), Err(ParseError::ConflictingLength));
        }

        #[test]
        fn duplicate_content_length() {
            check!("different values", one(b"POST / HTTP/1.1\r\nContent-Length: 4\r\nContent-Length: 5\r\n\r\nhello"), Err(ParseError::ConflictingLength));
            let r = one(b"POST / HTTP/1.1\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\nok").unwrap().unwrap();
            check!("identical values are fine", r.body, b"ok".to_vec());
        }

        #[test]
        fn content_length_is_digits_only() {
            for bad in ["+5", "-1", "0x10", "5 5", "", "99999999999999999999999"] {
                let req = format!("POST / HTTP/1.1\r\nContent-Length: {bad}\r\n\r\nhello");
                check!("rejected length", one(req.as_bytes()), Err(ParseError::BadContentLength));
            }
        }

        #[test]
        fn head_limit_applies_before_the_blank_line_arrives() {
            let mut p = Parser::new(Limits { max_head: 64, max_headers: 100, max_body: 100 });
            p.feed(b"GET / HTTP/1.1\r\n");
            check!("under the limit: wait", p.next_request(), Ok(None));
            p.feed(&[b'a'; 100]); // a header line that never ends
            check!("over the limit with no terminator", p.next_request(), Err(ParseError::HeadTooLarge));
        }

        #[test]
        fn head_limit_counts_a_complete_head_too() {
            let mut req = b"GET / HTTP/1.1\r\nX: ".to_vec();
            req.extend(vec![b'v'; 200]);
            req.extend(b"\r\n\r\n");
            let mut p = Parser::new(Limits { max_head: 100, max_headers: 100, max_body: 100 });
            p.feed(&req);
            check!("a complete but oversized head", p.next_request(), Err(ParseError::HeadTooLarge));
        }

        #[test]
        fn too_many_headers() {
            let mut req = b"GET / HTTP/1.1\r\n".to_vec();
            for i in 0..5 {
                req.extend(format!("H{i}: v\r\n").as_bytes());
            }
            req.extend(b"\r\n");
            let mut p = Parser::new(Limits { max_head: 8192, max_headers: 4, max_body: 100 });
            p.feed(&req);
            check!("five headers with a limit of four", p.next_request(), Err(ParseError::TooManyHeaders));
        }

        #[test]
        fn body_limit_fires_from_the_announced_length() {
            let mut p = Parser::new(Limits { max_head: 8192, max_headers: 8, max_body: 10 });
            p.feed(b"POST / HTTP/1.1\r\nContent-Length: 1000000000\r\n\r\n");
            check!("no body bytes have arrived yet", p.next_request(), Err(ParseError::BodyTooLarge));
            let mut q = Parser::new(Limits { max_head: 8192, max_headers: 8, max_body: 10 });
            q.feed(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFF\r\n");
            check!("a chunk announcing too much", q.next_request(), Err(ParseError::BodyTooLarge));
        }

        #[test]
        fn bad_chunks() {
            let head = b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n";
            for tail in [&b"zz\r\nab\r\n0\r\n\r\n"[..], b"2\r\nabc\r\n0\r\n\r\n", b"-1\r\n", b"\r\n", b"2;ok\r\nabXX0\r\n\r\n"] {
                let mut req = head.to_vec();
                req.extend(tail);
                check!("rejected chunk framing", one(&req), Err(ParseError::BadChunk));
            }
        }

        #[test]
        fn an_incomplete_body_consumes_nothing() {
            let mut p = Parser::new(lim());
            p.feed(b"POST / HTTP/1.1\r\nContent-Length: 10\r\n\r\nabc");
            let n = p.buffered();
            check!("wait for the rest", p.next_request(), Ok(None));
            check!("nothing consumed", p.buffered(), n);
            p.feed(b"defghij");
            let r = p.next_request().unwrap().unwrap();
            check!("whole body, buffer empty", (r.body, p.buffered()), (b"abcdefghij".to_vec(), 0));
        }

        #[test]
        fn splitting_the_bytes_anywhere_gives_the_same_requests() {
            let wire: Vec<u8> = [
                &b"GET /a?q=1 HTTP/1.1\r\nHost: h\r\nX-A: 1\r\n\r\n"[..],
                b"POST /p HTTP/1.1\r\nContent-Length: 11\r\n\r\nhello world",
                b"POST /c HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n4;x=y\r\ndefg\r\n0\r\n\r\n",
                b"GET /z HTTP/1.0\r\n\r\n",
            ]
            .concat();
            let whole = all(&[&wire], lim()).unwrap();
            check!("four requests", whole.len(), 4);
            let mut rng = anneal_prelude::Rng::new(6804);
            for _ in 0..300 {
                let mut cuts: Vec<usize> = (0..rng.below(12)).map(|_| rng.below(wire.len())).collect();
                cuts.sort();
                let mut pieces: Vec<&[u8]> = Vec::new();
                let mut last = 0;
                for c in cuts {
                    pieces.push(&wire[last..c]);
                    last = c;
                }
                pieces.push(&wire[last..]);
                check!("same requests however the bytes are split", all(&pieces, lim()).unwrap(), whole.clone());
            }
            let bytewise: Vec<&[u8]> = wire.chunks(1).collect();
            check!("one byte at a time", all(&bytewise, lim()).unwrap(), whole);
        }
