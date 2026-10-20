//! The frame codec of DD-005: `Request`, `Response`, `WireToken`, `ErrorKind`,
//! `frame_len` and `read_payload`.
//!
//! The byte fixtures pin the layout byte for byte (little-endian, a `u32`
//! length before every string, `TDBP` before the version in a hello), so a
//! change to the wire format fails here. The rest feeds the decoder hostile
//! input: it must return the specific `ProtocolError` and never panic.
//!
//! `ProtocolError` holds an `io::Error`, so it is not `PartialEq`; errors are
//! matched with `assert_err!`. Only the public API is used.

use std::error::Error;
use std::io::{self, Cursor, Read, Write};
use std::time::{Duration, Instant};

use tdb_lexer::{Lexer, Model, Token, TokenType};
use tdb_protocol::{
    ErrorKind, MAGIC, MAX_FRAME_LEN, MAX_HELLO_LEN, MAX_STATEMENT_LEN, ProtocolError, Request,
    Response, Route, RouteError, VERSION, WireToken, frame_len, frame_len_within, hello_version,
    read_payload,
};

/// `MAX_FRAME_LEN + 1`: the smallest length that is refused.
const OVER_MAX: u64 = MAX_FRAME_LEN as u64 + 1;

/// Nothing hostile may take anywhere near this long, even in a debug build on
/// a busy machine.
const QUICKLY: Duration = Duration::from_secs(5);

/// Asserts that `$result` is `Err($pattern)`, and shows what it was if not.
macro_rules! assert_err {
    ($result:expr, $pattern:pat) => {
        match $result {
            Err($pattern) => {}
            other => panic!("expected Err({}), got {other:?}", stringify!($pattern)),
        }
    };
}

// --- Helpers ---

/// Glues byte pieces together, so a fixture reads as its fields.
fn bytes(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

/// A string field: a `u32` byte length, then the bytes.
fn string_field(s: &[u8]) -> Vec<u8> {
    let len = u32::try_from(s.len()).expect("test string fits in a u32");
    bytes(&[&len.to_le_bytes(), s])
}

fn route(path: &str) -> Route {
    path.parse().expect("valid route")
}

fn token(kind: &str, value: &str, line: u32, column: u32) -> WireToken {
    WireToken {
        kind: kind.to_owned(),
        value: value.to_owned(),
        line,
        column,
    }
}

/// The body of a hello with `route` as its (unchecked) route bytes.
fn hello_body(version: u16, route: &[u8]) -> Vec<u8> {
    bytes(&[
        &[0x01],
        b"TDBP",
        &version.to_le_bytes(),
        &string_field(route),
    ])
}

/// The body of a tokens response with `count` as its claimed token count,
/// followed by `rest`.
fn tokens_body(count: u32, rest: &[u8]) -> Vec<u8> {
    bytes(&[&[0x82], &count.to_le_bytes(), rest])
}

/// Runs `f` and fails the test if it took long.
fn quickly<T>(f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    assert!(start.elapsed() < QUICKLY, "took {:?}", start.elapsed());
    out
}

fn hello_route_error(route_bytes: &[u8]) -> RouteError {
    match Request::from_payload(&hello_body(1, route_bytes)) {
        Err(ProtocolError::Route(e)) => e,
        other => panic!("expected ProtocolError::Route for {route_bytes:?}, got {other:?}"),
    }
}

struct Fixture<M> {
    name: &'static str,
    message: M,
    frame: Vec<u8>,
}

impl<M> Fixture<M> {
    /// The frame without its 4-byte header: what `read_payload` returns.
    fn body(&self) -> &[u8] {
        &self.frame[4..]
    }
}

fn request_fixtures() -> Vec<Fixture<Request>> {
    vec![
        Fixture {
            name: "hello",
            message: Request::Hello {
                version: 1,
                route: route("/graph/social"),
            },
            frame: bytes(&[
                &[0x18, 0, 0, 0], // body: 24 bytes
                &[0x01],          // Hello
                b"TDBP",          // magic
                &[0x01, 0x00],    // version 1
                &[0x0d, 0, 0, 0], // route: 13 bytes
                b"/graph/social",
            ]),
        },
        Fixture {
            name: "hello_version_is_little_endian",
            message: Request::Hello {
                version: 0x0102,
                route: route("/keyval/main"),
            },
            frame: bytes(&[
                &[0x17, 0, 0, 0], // body: 23 bytes
                &[0x01],
                b"TDBP",
                &[0x02, 0x01], // 0x0102, low byte first
                &[0x0c, 0, 0, 0],
                b"/keyval/main",
            ]),
        },
        Fixture {
            name: "query",
            message: Request::Query {
                statement: "GET user:1".to_owned(),
            },
            frame: bytes(&[
                &[0x0f, 0, 0, 0], // body: 15 bytes
                &[0x02],          // Query
                &[0x0a, 0, 0, 0], // statement: 10 bytes
                b"GET user:1",
            ]),
        },
        Fixture {
            name: "query_empty",
            message: Request::Query {
                statement: String::new(),
            },
            frame: bytes(&[&[0x05, 0, 0, 0], &[0x02], &[0, 0, 0, 0]]),
        },
        Fixture {
            name: "query_length_counts_bytes_not_characters",
            message: Request::Query {
                statement: "\u{65e5}\u{672c}".to_owned(),
            },
            frame: bytes(&[
                &[0x0b, 0, 0, 0], // body: 11 bytes
                &[0x02],
                &[0x06, 0, 0, 0], // two characters, six bytes
                &[0xe6, 0x97, 0xa5, 0xe6, 0x9c, 0xac],
            ]),
        },
        Fixture {
            name: "query_length_is_four_bytes_little_endian",
            message: Request::Query {
                statement: "a".repeat(300),
            },
            frame: bytes(&[
                &[0x31, 0x01, 0, 0], // body: 305 bytes
                &[0x02],
                &[0x2c, 0x01, 0, 0], // statement: 300 bytes
                &[b'a'; 300],
            ]),
        },
    ]
}

fn response_fixtures() -> Vec<Fixture<Response>> {
    let mut fixtures = vec![
        Fixture {
            name: "hello_ok",
            message: Response::HelloOk { version: 1 },
            frame: bytes(&[
                &[0x03, 0, 0, 0], // body: 3 bytes
                &[0x81],          // HelloOk
                &[0x01, 0x00],    // version 1
            ]),
        },
        Fixture {
            name: "hello_ok_version_is_little_endian",
            message: Response::HelloOk { version: 0x0102 },
            frame: bytes(&[&[0x03, 0, 0, 0], &[0x81], &[0x02, 0x01]]),
        },
        Fixture {
            name: "tokens",
            message: Response::Tokens(vec![
                token("Keyword(GET)", "GET", 1, 1),
                token("Word", "user:1", 1, 5),
            ]),
            frame: bytes(&[
                &[0x3e, 0, 0, 0], // body: 62 bytes
                &[0x82],          // Tokens
                &[0x02, 0, 0, 0], // two tokens
                &[0x0c, 0, 0, 0], // kind: 12 bytes
                b"Keyword(GET)",  //
                &[0x03, 0, 0, 0], // value: 3 bytes
                b"GET",           //
                &[0x01, 0, 0, 0], // line 1
                &[0x01, 0, 0, 0], // column 1
                &[0x04, 0, 0, 0], // kind: 4 bytes
                b"Word",          //
                &[0x06, 0, 0, 0], // value: 6 bytes
                b"user:1",        //
                &[0x01, 0, 0, 0], // line 1
                &[0x05, 0, 0, 0], // column 5
            ]),
        },
        Fixture {
            name: "tokens_empty",
            message: Response::Tokens(Vec::new()),
            frame: bytes(&[&[0x05, 0, 0, 0], &[0x82], &[0, 0, 0, 0]]),
        },
        Fixture {
            name: "tokens_numbers_are_little_endian",
            message: Response::Tokens(vec![token("Integer", "7", 0x0102_0304, 258)]),
            frame: bytes(&[
                &[0x1d, 0, 0, 0], // body: 29 bytes
                &[0x82],
                &[0x01, 0, 0, 0],
                &[0x07, 0, 0, 0],
                b"Integer",
                &[0x01, 0, 0, 0],
                b"7",
                &[0x04, 0x03, 0x02, 0x01], // line 0x01020304
                &[0x02, 0x01, 0x00, 0x00], // column 258
            ]),
        },
        Fixture {
            name: "tokens_empty_strings_and_largest_position",
            message: Response::Tokens(vec![token("", "", u32::MAX, u32::MAX)]),
            frame: bytes(&[
                &[0x15, 0, 0, 0], // body: 21 bytes
                &[0x82],
                &[0x01, 0, 0, 0],
                &[0, 0, 0, 0], // kind: empty
                &[0, 0, 0, 0], // value: empty
                &[0xff, 0xff, 0xff, 0xff],
                &[0xff, 0xff, 0xff, 0xff],
            ]),
        },
        Fixture {
            name: "error_empty_message",
            message: Response::error(ErrorKind::Lex, ""),
            frame: bytes(&[
                &[0x06, 0, 0, 0], // body: 6 bytes
                &[0xff],          // Error
                &[0x04],          // kind: Lex
                &[0, 0, 0, 0],    // message: empty
            ]),
        },
    ];
    let errors: [(&'static str, ErrorKind, [u8; 14]); 5] = [
        (
            "error_protocol",
            ErrorKind::Protocol,
            [
                0x0a, 0, 0, 0, 0xff, 0x01, 0x04, 0, 0, 0, b'o', b'o', b'p', b's',
            ],
        ),
        (
            "error_version",
            ErrorKind::Version,
            [
                0x0a, 0, 0, 0, 0xff, 0x02, 0x04, 0, 0, 0, b'o', b'o', b'p', b's',
            ],
        ),
        (
            "error_route",
            ErrorKind::Route,
            [
                0x0a, 0, 0, 0, 0xff, 0x03, 0x04, 0, 0, 0, b'o', b'o', b'p', b's',
            ],
        ),
        (
            "error_lex",
            ErrorKind::Lex,
            [
                0x0a, 0, 0, 0, 0xff, 0x04, 0x04, 0, 0, 0, b'o', b'o', b'p', b's',
            ],
        ),
        (
            "error_too_large",
            ErrorKind::TooLarge,
            [
                0x0a, 0, 0, 0, 0xff, 0x05, 0x04, 0, 0, 0, b'o', b'o', b'p', b's',
            ],
        ),
    ];
    for (name, kind, frame) in errors {
        fixtures.push(Fixture {
            name,
            message: Response::error(kind, "oops"),
            frame: frame.to_vec(),
        });
    }
    fixtures
}

fn fixture(name: &str) -> Fixture<Response> {
    response_fixtures()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no response fixture {name:?}"))
}

fn request_fixture(name: &str) -> Fixture<Request> {
    request_fixtures()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no request fixture {name:?}"))
}

/// The frame is exactly `expected`, and decoding its body gives the message
/// back.
#[track_caller]
fn assert_request_layout(message: &Request, expected: &[u8]) {
    let frame = message.to_frame().expect("encode request");
    assert_eq!(frame, expected, "frame of {message:?}");
    let decoded = Request::from_payload(&frame[4..]).expect("decode request");
    assert_eq!(&decoded, message);
}

#[track_caller]
fn assert_response_layout(message: &Response, expected: &[u8]) {
    let frame = message.to_frame().expect("encode response");
    assert_eq!(frame, expected, "frame of {message:?}");
    let decoded = Response::from_payload(&frame[4..]).expect("decode response");
    assert_eq!(&decoded, message);
}

/// Encodes then decodes, checking the header against the body on the way.
#[track_caller]
fn round_trip_request(message: &Request) {
    let frame = message.to_frame().expect("encode request");
    let header = u32::from_le_bytes(frame[..4].try_into().unwrap());
    assert_eq!(header as usize, frame.len() - 4, "header of {message:?}");
    assert_eq!(&Request::from_payload(&frame[4..]).unwrap(), message);
}

#[track_caller]
fn round_trip_response(message: &Response) {
    let frame = message.to_frame().expect("encode response");
    let header = u32::from_le_bytes(frame[..4].try_into().unwrap());
    assert_eq!(header as usize, frame.len() - 4, "header of {message:?}");
    assert_eq!(&Response::from_payload(&frame[4..]).unwrap(), message);
}

// --- (a) The layout, byte for byte ---

#[test]
fn codec_constants_match_the_spec() {
    assert_eq!(MAGIC, *b"TDBP");
    assert_eq!(VERSION, 1);
    assert_eq!(MAX_FRAME_LEN, 4 * 1024 * 1024);
    assert_eq!(MAX_HELLO_LEN, 256);
    assert_eq!(MAX_STATEMENT_LEN, 64 * 1024);
}

#[test]
fn codec_hello_layout() {
    let f = request_fixture("hello");
    assert_request_layout(&f.message, &f.frame);
}

#[test]
fn codec_hello_version_is_little_endian() {
    let f = request_fixture("hello_version_is_little_endian");
    assert_request_layout(&f.message, &f.frame);
}

#[test]
fn codec_hello_route_is_the_path_form() {
    for (model, name) in [
        (Model::Keyval, "keyval"),
        (Model::Rdbms, "rdbms"),
        (Model::Vector, "vector"),
        (Model::Graph, "graph"),
    ] {
        let path = format!("/{name}/db_1");
        let hello = Request::Hello {
            version: 1,
            route: Route::new(model, "db_1").unwrap(),
        };
        let expected = hello_body(1, path.as_bytes());
        let frame = hello.to_frame().unwrap();
        assert_eq!(&frame[4..], &expected[..], "hello for {path}");
    }
}

#[test]
fn codec_query_layout() {
    let f = request_fixture("query");
    assert_request_layout(&f.message, &f.frame);
}

#[test]
fn codec_query_empty_layout() {
    let f = request_fixture("query_empty");
    assert_request_layout(&f.message, &f.frame);
}

#[test]
fn codec_query_length_counts_bytes_not_characters() {
    let f = request_fixture("query_length_counts_bytes_not_characters");
    assert_request_layout(&f.message, &f.frame);
}

#[test]
fn codec_query_length_is_four_bytes_little_endian() {
    let f = request_fixture("query_length_is_four_bytes_little_endian");
    assert_request_layout(&f.message, &f.frame);
}

#[test]
fn codec_hello_ok_layout() {
    let f = fixture("hello_ok");
    assert_response_layout(&f.message, &f.frame);
}

#[test]
fn codec_hello_ok_version_is_little_endian() {
    let f = fixture("hello_ok_version_is_little_endian");
    assert_response_layout(&f.message, &f.frame);
}

#[test]
fn codec_tokens_layout_with_two_tokens() {
    let f = fixture("tokens");
    assert_response_layout(&f.message, &f.frame);
}

#[test]
fn codec_tokens_empty_layout() {
    let f = fixture("tokens_empty");
    assert_response_layout(&f.message, &f.frame);
}

#[test]
fn codec_tokens_numbers_are_little_endian() {
    let f = fixture("tokens_numbers_are_little_endian");
    assert_response_layout(&f.message, &f.frame);
}

#[test]
fn codec_tokens_empty_strings_and_largest_position_layout() {
    let f = fixture("tokens_empty_strings_and_largest_position");
    assert_response_layout(&f.message, &f.frame);
}

#[test]
fn codec_error_layout_for_each_kind() {
    for name in [
        "error_protocol",
        "error_version",
        "error_route",
        "error_lex",
        "error_too_large",
    ] {
        let f = fixture(name);
        assert_response_layout(&f.message, &f.frame);
    }
}

#[test]
fn codec_error_kind_byte_is_the_code_in_the_spec() {
    // 1 Protocol, 2 Version, 3 Route, 4 Lex, 5 TooLarge: the byte after 0xFF.
    for (kind, code) in [
        (ErrorKind::Protocol, 1),
        (ErrorKind::Version, 2),
        (ErrorKind::Route, 3),
        (ErrorKind::Lex, 4),
        (ErrorKind::TooLarge, 5),
    ] {
        let frame = Response::error(kind, "m").to_frame().unwrap();
        assert_eq!(frame[4], 0xff, "type byte for {kind:?}");
        assert_eq!(frame[5], code, "kind byte for {kind:?}");
    }
}

#[test]
fn codec_error_empty_message_layout() {
    let f = fixture("error_empty_message");
    assert_response_layout(&f.message, &f.frame);
}

#[test]
fn codec_requests_are_below_0x80_and_responses_are_not() {
    for f in request_fixtures() {
        assert!(f.body()[0] < 0x80, "{}", f.name);
    }
    for f in response_fixtures() {
        assert!(f.body()[0] >= 0x80, "{}", f.name);
    }
}

#[test]
fn codec_every_fixture_header_is_its_body_length() {
    for f in request_fixtures() {
        let header = u32::from_le_bytes(f.frame[..4].try_into().unwrap());
        assert_eq!(header as usize, f.body().len(), "{}", f.name);
    }
    for f in response_fixtures() {
        let header = u32::from_le_bytes(f.frame[..4].try_into().unwrap());
        assert_eq!(header as usize, f.body().len(), "{}", f.name);
    }
}

#[test]
fn codec_response_error_constructor_builds_the_variant() {
    assert_eq!(
        Response::error(ErrorKind::TooLarge, "big"),
        Response::Error {
            kind: ErrorKind::TooLarge,
            message: "big".to_owned(),
        }
    );
    assert_eq!(
        Response::error(ErrorKind::Lex, String::from("owned")),
        Response::Error {
            kind: ErrorKind::Lex,
            message: "owned".to_owned(),
        }
    );
}

// --- (b) Round trips of edge values ---

#[test]
fn codec_round_trips_an_empty_statement() {
    round_trip_request(&Request::Query {
        statement: String::new(),
    });
}

#[test]
fn codec_round_trips_multi_byte_utf8() {
    for statement in [
        "h\u{e9}llo w\u{f6}rld",
        "\u{65e5}\u{672c}\u{8a9e}",
        "\u{1f980} crab",
        "\u{10ffff}",
        "a\u{300}",
    ] {
        round_trip_request(&Request::Query {
            statement: statement.to_owned(),
        });
    }
}

#[test]
fn codec_round_trips_control_characters_in_a_statement() {
    round_trip_request(&Request::Query {
        statement: "SET a\0b\r\nSET c\td\n".to_owned(),
    });
}

#[test]
fn codec_round_trips_a_statement_of_the_largest_accepted_size() {
    round_trip_request(&Request::Query {
        statement: "x".repeat(MAX_STATEMENT_LEN),
    });
}

#[test]
fn codec_round_trips_every_model_and_the_longest_database() {
    let longest = "d".repeat(64);
    for model in [Model::Keyval, Model::Rdbms, Model::Vector, Model::Graph] {
        for database in ["a", "main", longest.as_str()] {
            round_trip_request(&Request::Hello {
                version: VERSION,
                route: Route::new(model, database).unwrap(),
            });
        }
    }
}

#[test]
fn codec_round_trips_any_hello_version() {
    // Whether a version is acceptable is the server's call, not the codec's.
    for version in [0, 1, 2, 255, 256, 0x0102, u16::MAX] {
        round_trip_request(&Request::Hello {
            version,
            route: route("/graph/a"),
        });
        round_trip_response(&Response::HelloOk { version });
    }
}

#[test]
fn codec_round_trips_an_empty_token_list() {
    round_trip_response(&Response::Tokens(Vec::new()));
}

#[test]
fn codec_round_trips_the_largest_line_and_column() {
    round_trip_response(&Response::Tokens(vec![
        token("Word", "x", u32::MAX, u32::MAX),
        token("Word", "y", 0, 0),
        token("Word", "z", u32::MAX, 1),
    ]));
}

#[test]
fn codec_round_trips_empty_and_multi_byte_token_strings() {
    round_trip_response(&Response::Tokens(vec![
        token("", "", 1, 1),
        token("String", "caf\u{e9} \u{65e5}\u{672c} \u{1f980}", 2, 3),
        token("Word", "with\0nul and\nnewline", 4, 5),
    ]));
}

#[test]
fn codec_round_trips_many_tokens() {
    let tokens: Vec<WireToken> = (0..10_000u32)
        .map(|i| token("Word", &format!("w{i}"), i / 80 + 1, i % 80 + 1))
        .collect();
    round_trip_response(&Response::Tokens(tokens));
}

#[test]
fn codec_round_trips_every_error_kind_and_message() {
    for kind in [
        ErrorKind::Protocol,
        ErrorKind::Version,
        ErrorKind::Route,
        ErrorKind::Lex,
        ErrorKind::TooLarge,
    ] {
        for message in ["", "x", "unexpected `)` at 1:9", "\u{e9}\u{65e5}\u{1f980}"] {
            round_trip_response(&Response::error(kind, message));
        }
    }
}

#[test]
fn codec_every_fixture_round_trips() {
    for f in request_fixtures() {
        assert_eq!(
            Request::from_payload(f.body()).unwrap(),
            f.message,
            "{}",
            f.name
        );
    }
    for f in response_fixtures() {
        assert_eq!(
            Response::from_payload(f.body()).unwrap(),
            f.message,
            "{}",
            f.name
        );
    }
}

// --- (c) Hostile and malformed bodies ---

#[test]
fn codec_empty_request_payload_is_truncated() {
    assert_err!(Request::from_payload(&[]), ProtocolError::Truncated);
}

#[test]
fn codec_empty_response_payload_is_truncated() {
    assert_err!(Response::from_payload(&[]), ProtocolError::Truncated);
}

#[test]
fn codec_every_strict_prefix_of_a_request_is_truncated() {
    for f in request_fixtures() {
        for len in 0..f.body().len() {
            let result = Request::from_payload(&f.body()[..len]);
            assert!(
                matches!(result, Err(ProtocolError::Truncated)),
                "{}: prefix of {len} of {} bytes: expected Truncated, got {result:?}",
                f.name,
                f.body().len()
            );
        }
    }
}

#[test]
fn codec_every_strict_prefix_of_a_response_is_truncated() {
    for f in response_fixtures() {
        for len in 0..f.body().len() {
            let result = Response::from_payload(&f.body()[..len]);
            assert!(
                matches!(result, Err(ProtocolError::Truncated)),
                "{}: prefix of {len} of {} bytes: expected Truncated, got {result:?}",
                f.name,
                f.body().len()
            );
        }
    }
}

#[test]
fn codec_every_strict_prefix_of_many_tokens_is_truncated() {
    let tokens: Vec<WireToken> = (0..20u32)
        .map(|i| token("Word", &format!("w\u{e9}{i}"), 1, i + 1))
        .collect();
    let frame = Response::Tokens(tokens).to_frame().unwrap();
    for len in 0..frame.len() - 4 {
        let result = Response::from_payload(&frame[4..4 + len]);
        assert!(
            matches!(result, Err(ProtocolError::Truncated)),
            "prefix of {len} bytes: expected Truncated, got {result:?}"
        );
    }
}

#[test]
fn codec_unknown_request_tags_are_unknown_tag() {
    assert_err!(
        Request::from_payload(&[0x00]),
        ProtocolError::UnknownTag(0x00)
    );
    assert_err!(
        Request::from_payload(&[0x03]),
        ProtocolError::UnknownTag(0x03)
    );
    assert_err!(
        Request::from_payload(&[0x7f]),
        ProtocolError::UnknownTag(0x7f)
    );
    assert_err!(
        Request::from_payload(&[0x80]),
        ProtocolError::UnknownTag(0x80)
    );
    assert_err!(
        Request::from_payload(&[0x03, 1, 2, 3]),
        ProtocolError::UnknownTag(0x03)
    );
}

#[test]
fn codec_unknown_response_tags_are_unknown_tag() {
    assert_err!(
        Response::from_payload(&[0x00]),
        ProtocolError::UnknownTag(0x00)
    );
    assert_err!(
        Response::from_payload(&[0x7f]),
        ProtocolError::UnknownTag(0x7f)
    );
    assert_err!(
        Response::from_payload(&[0x80]),
        ProtocolError::UnknownTag(0x80)
    );
    assert_err!(
        Response::from_payload(&[0x83]),
        ProtocolError::UnknownTag(0x83)
    );
    assert_err!(
        Response::from_payload(&[0xfe]),
        ProtocolError::UnknownTag(0xfe)
    );
    assert_err!(
        Response::from_payload(&[0x83, 1, 2, 3]),
        ProtocolError::UnknownTag(0x83)
    );
}

#[test]
fn codec_a_lone_tag_byte_is_unknown_or_truncated_for_every_value() {
    for tag in 0..=u8::MAX {
        match (tag, Request::from_payload(&[tag])) {
            (0x01 | 0x02, Err(ProtocolError::Truncated)) => {}
            (0x01 | 0x02, other) => panic!("request tag {tag:#04x}: got {other:?}"),
            (_, Err(ProtocolError::UnknownTag(got))) if got == tag => {}
            (_, other) => panic!("request tag {tag:#04x}: got {other:?}"),
        }
        match (tag, Response::from_payload(&[tag])) {
            (0x81 | 0x82 | 0xff, Err(ProtocolError::Truncated)) => {}
            (0x81 | 0x82 | 0xff, other) => panic!("response tag {tag:#04x}: got {other:?}"),
            (_, Err(ProtocolError::UnknownTag(got))) if got == tag => {}
            (_, other) => panic!("response tag {tag:#04x}: got {other:?}"),
        }
    }
}

#[test]
fn codec_a_request_where_a_response_is_expected_is_unknown_tag() {
    let hello = request_fixture("hello");
    assert_err!(
        Response::from_payload(hello.body()),
        ProtocolError::UnknownTag(0x01)
    );
    let query = request_fixture("query");
    assert_err!(
        Response::from_payload(query.body()),
        ProtocolError::UnknownTag(0x02)
    );
}

#[test]
fn codec_a_response_where_a_request_is_expected_is_unknown_tag() {
    assert_err!(
        Request::from_payload(fixture("hello_ok").body()),
        ProtocolError::UnknownTag(0x81)
    );
    assert_err!(
        Request::from_payload(fixture("tokens").body()),
        ProtocolError::UnknownTag(0x82)
    );
    assert_err!(
        Request::from_payload(fixture("error_lex").body()),
        ProtocolError::UnknownTag(0xff)
    );
}

#[test]
fn codec_a_hello_with_the_wrong_magic_is_bad_magic() {
    for magic in [
        b"TDBX",
        b"tdbp",
        b"HTTP",
        b"PDBT",
        b"\0\0\0\0",
        b"TDB\0",
        b"XTDB",
    ] {
        let body = bytes(&[
            &[0x01],
            magic,
            &[0x01, 0x00],
            &string_field(b"/graph/social"),
        ]);
        assert_err!(Request::from_payload(&body), ProtocolError::BadMagic);
    }
}

#[test]
fn codec_a_hello_that_is_an_http_request_is_bad_magic() {
    // The hello's type byte is 0x01; real HTTP does not even get that far
    // (see the frame tests), but a body that does must still be turned away.
    let body = bytes(&[&[0x01], b"GET / HTTP/1.1\r\n\r\n"]);
    assert_err!(Request::from_payload(&body), ProtocolError::BadMagic);
}

#[test]
fn codec_bad_utf8_in_a_statement_is_bad_utf8() {
    for bad in [
        &[0xff, 0xfe][..],
        &[0xc0, 0x80],             // overlong NUL
        &[0xed, 0xa0, 0x80],       // a surrogate
        &[0xe6, 0x97],             // a character cut short inside the string
        &[0x80],                   // a continuation byte alone
        &[0xf4, 0x90, 0x80, 0x80], // above U+10FFFF
    ] {
        let body = bytes(&[&[0x02], &string_field(bad)]);
        let result = Request::from_payload(&body);
        assert!(
            matches!(result, Err(ProtocolError::BadUtf8)),
            "{bad:02x?}: expected BadUtf8, got {result:?}"
        );
    }
}

#[test]
fn codec_bad_utf8_in_a_hello_route_is_bad_utf8() {
    assert_err!(
        Request::from_payload(&hello_body(1, &[b'/', 0xff, b'/', b'a'])),
        ProtocolError::BadUtf8
    );
}

#[test]
fn codec_bad_utf8_in_a_token_kind_is_bad_utf8() {
    let rest = bytes(&[
        &string_field(&[0xff]),
        &string_field(b"v"),
        &[1, 0, 0, 0],
        &[1, 0, 0, 0],
    ]);
    assert_err!(
        Response::from_payload(&tokens_body(1, &rest)),
        ProtocolError::BadUtf8
    );
}

#[test]
fn codec_bad_utf8_in_a_token_value_is_bad_utf8() {
    let rest = bytes(&[
        &string_field(b"Word"),
        &string_field(&[0xc3]),
        &[1, 0, 0, 0],
        &[1, 0, 0, 0],
    ]);
    assert_err!(
        Response::from_payload(&tokens_body(1, &rest)),
        ProtocolError::BadUtf8
    );
}

#[test]
fn codec_bad_utf8_in_an_error_message_is_bad_utf8() {
    let body = bytes(&[&[0xff, 0x04], &string_field(&[0xff, 0xff])]);
    assert_err!(Response::from_payload(&body), ProtocolError::BadUtf8);
}

#[test]
fn codec_trailing_bytes_after_a_request_are_counted() {
    for f in request_fixtures() {
        let one = bytes(&[f.body(), &[0xaa]]);
        let result = Request::from_payload(&one);
        assert!(
            matches!(result, Err(ProtocolError::TrailingBytes(1))),
            "{}: expected TrailingBytes(1), got {result:?}",
            f.name
        );
        let seven = bytes(&[f.body(), &[0; 7]]);
        let result = Request::from_payload(&seven);
        assert!(
            matches!(result, Err(ProtocolError::TrailingBytes(7))),
            "{}: expected TrailingBytes(7), got {result:?}",
            f.name
        );
    }
}

#[test]
fn codec_trailing_bytes_after_a_response_are_counted() {
    for f in response_fixtures() {
        let one = bytes(&[f.body(), &[0xaa]]);
        let result = Response::from_payload(&one);
        assert!(
            matches!(result, Err(ProtocolError::TrailingBytes(1))),
            "{}: expected TrailingBytes(1), got {result:?}",
            f.name
        );
        let seven = bytes(&[f.body(), &[0; 7]]);
        let result = Response::from_payload(&seven);
        assert!(
            matches!(result, Err(ProtocolError::TrailingBytes(7))),
            "{}: expected TrailingBytes(7), got {result:?}",
            f.name
        );
    }
}

#[test]
fn codec_trailing_bytes_after_a_whole_second_message_are_counted() {
    // Two messages in one body are not a body: the frame holds exactly one.
    let query = request_fixture("query");
    let doubled = bytes(&[query.body(), query.body()]);
    assert_err!(
        Request::from_payload(&doubled),
        ProtocolError::TrailingBytes(15)
    );
}

#[test]
fn codec_a_string_shorter_than_its_data_leaves_trailing_bytes() {
    let body = bytes(&[&[0x02], &[0x01, 0, 0, 0], b"ab"]);
    assert_err!(
        Request::from_payload(&body),
        ProtocolError::TrailingBytes(1)
    );
}

#[test]
fn codec_a_string_longer_than_its_data_is_truncated() {
    let body = bytes(&[&[0x02], &[0x03, 0, 0, 0], b"ab"]);
    assert_err!(Request::from_payload(&body), ProtocolError::Truncated);
}

#[test]
fn codec_unknown_error_kind_codes_are_unknown_error_kind() {
    assert_err!(
        Response::from_payload(&bytes(&[&[0xff, 0x00], &string_field(b"m")])),
        ProtocolError::UnknownErrorKind(0)
    );
    assert_err!(
        Response::from_payload(&bytes(&[&[0xff, 0x06], &string_field(b"m")])),
        ProtocolError::UnknownErrorKind(6)
    );
    assert_err!(
        Response::from_payload(&bytes(&[&[0xff, 0xff], &string_field(b"m")])),
        ProtocolError::UnknownErrorKind(0xff)
    );
}

#[test]
fn codec_every_error_kind_code_outside_1_to_5_is_unknown_error_kind() {
    for code in (0..=u8::MAX).filter(|c| !(1..=5).contains(c)) {
        let body = bytes(&[&[0xff, code], &string_field(b"oops")]);
        match Response::from_payload(&body) {
            Err(ProtocolError::UnknownErrorKind(got)) if got == code => {}
            other => panic!("code {code}: expected UnknownErrorKind({code}), got {other:?}"),
        }
    }
}

#[test]
fn codec_a_hello_with_an_invalid_route_is_a_route_error() {
    let long = format!("/graph/{}", "d".repeat(65));
    let cases: [(&[u8], RouteError); 10] = [
        (b"/graph", RouteError::MissingDatabase),
        (b"/", RouteError::MissingDatabase),
        (b"graph/a", RouteError::MissingSlash),
        (b"", RouteError::MissingSlash),
        (b"/graph/", RouteError::EmptyDatabase),
        (b"/graph/a/b", RouteError::ExtraSegments),
        (b"/nope/a", RouteError::UnknownModel("nope".to_owned())),
        (b"/GRAPH/a", RouteError::UnknownModel("GRAPH".to_owned())),
        (b"/graph/a.b", RouteError::BadDatabaseChar('.')),
        (b"/graph/x?y=1", RouteError::BadDatabaseChar('?')),
    ];
    for (path, expected) in cases {
        assert_eq!(hello_route_error(path), expected, "route {:?}", path);
    }
    assert_eq!(
        hello_route_error(long.as_bytes()),
        RouteError::DatabaseTooLong(65)
    );
}

#[test]
fn codec_a_hello_route_that_climbs_out_of_the_data_directory_is_rejected() {
    assert_eq!(
        hello_route_error(b"/graph/.."),
        RouteError::BadDatabaseChar('.')
    );
    assert_eq!(
        hello_route_error(b"/graph/../../etc"),
        RouteError::ExtraSegments
    );
    assert_eq!(
        hello_route_error(b"/graph/a\0b"),
        RouteError::BadDatabaseChar('\0')
    );
}

#[test]
fn codec_a_hello_with_a_hostile_model_name_echoes_at_most_32_characters() {
    let path = format!("/{}/db", "m".repeat(100_000));
    assert_eq!(
        hello_route_error(path.as_bytes()),
        RouteError::UnknownModel("m".repeat(32))
    );
}

#[test]
fn codec_a_hello_with_an_invalid_route_is_reported_as_a_route_error_kind() {
    let err = Request::from_payload(&hello_body(1, b"/graph")).unwrap_err();
    assert_eq!(ErrorKind::of(&err), ErrorKind::Route);
}

#[test]
fn codec_a_tokens_count_larger_than_the_data_is_truncated() {
    let f = fixture("tokens");
    // The same two tokens, but the count says three.
    let mut body = f.body().to_vec();
    body[1] = 3;
    assert_err!(Response::from_payload(&body), ProtocolError::Truncated);
}

#[test]
fn codec_a_tokens_count_smaller_than_the_data_leaves_trailing_bytes() {
    let f = fixture("tokens");
    // The same two tokens, but the count says one: the second token's 26 bytes
    // are left over.
    let mut body = f.body().to_vec();
    body[1] = 1;
    assert_err!(
        Response::from_payload(&body),
        ProtocolError::TrailingBytes(26)
    );
}

#[test]
fn codec_a_tokens_count_of_zero_with_data_leaves_trailing_bytes() {
    let body = tokens_body(0, &[1, 2, 3]);
    assert_err!(
        Response::from_payload(&body),
        ProtocolError::TrailingBytes(3)
    );
}

#[test]
fn codec_a_tokens_count_of_u32_max_with_no_data_is_truncated_at_once() {
    let body = tokens_body(u32::MAX, &[]);
    let result = quickly(|| Response::from_payload(&body));
    assert!(
        matches!(result, Err(ProtocolError::Truncated)),
        "expected Truncated, got {result:?}"
    );
}

#[test]
fn codec_a_tokens_count_of_u32_max_with_some_data_is_truncated_at_once() {
    let f = fixture("tokens");
    let body = tokens_body(u32::MAX, &f.body()[5..]);
    let result = quickly(|| Response::from_payload(&body));
    assert!(
        matches!(result, Err(ProtocolError::Truncated)),
        "expected Truncated, got {result:?}"
    );
}

#[test]
fn codec_a_tokens_count_in_the_millions_with_no_data_is_truncated_at_once() {
    for count in [1_000_000, 100_000_000, u32::MAX - 1, u32::MAX / 2] {
        let body = tokens_body(count, &[]);
        let result = quickly(|| Response::from_payload(&body));
        assert!(
            matches!(result, Err(ProtocolError::Truncated)),
            "count {count}: expected Truncated, got {result:?}"
        );
    }
}

#[test]
fn codec_a_string_length_of_u32_max_with_no_data_is_truncated_at_once() {
    let max = &u32::MAX.to_le_bytes();

    let query = bytes(&[&[0x02], max]);
    let result = quickly(|| Request::from_payload(&query));
    assert!(
        matches!(result, Err(ProtocolError::Truncated)),
        "query: expected Truncated, got {result:?}"
    );

    let hello = bytes(&[&[0x01], b"TDBP", &[0x01, 0x00], max]);
    let result = quickly(|| Request::from_payload(&hello));
    assert!(
        matches!(result, Err(ProtocolError::Truncated)),
        "hello: expected Truncated, got {result:?}"
    );

    let token_kind = tokens_body(1, max);
    let result = quickly(|| Response::from_payload(&token_kind));
    assert!(
        matches!(result, Err(ProtocolError::Truncated)),
        "token kind: expected Truncated, got {result:?}"
    );

    let error = bytes(&[&[0xff, 0x01], max]);
    let result = quickly(|| Response::from_payload(&error));
    assert!(
        matches!(result, Err(ProtocolError::Truncated)),
        "error message: expected Truncated, got {result:?}"
    );
}

#[test]
fn codec_a_string_length_of_u32_max_with_some_data_is_truncated_at_once() {
    let body = bytes(&[&[0x02], &u32::MAX.to_le_bytes(), &[b'a'; 1000]]);
    let result = quickly(|| Request::from_payload(&body));
    assert!(
        matches!(result, Err(ProtocolError::Truncated)),
        "expected Truncated, got {result:?}"
    );
}

#[test]
fn codec_a_token_value_length_of_u32_max_is_truncated_at_once() {
    let rest = bytes(&[&string_field(b"Word"), &u32::MAX.to_le_bytes()]);
    let result = quickly(|| Response::from_payload(&tokens_body(1, &rest)));
    assert!(
        matches!(result, Err(ProtocolError::Truncated)),
        "expected Truncated, got {result:?}"
    );
}

// --- (d) frame_len and frames that are too big to send ---

#[test]
fn codec_frame_len_zero_is_ok() {
    assert_eq!(frame_len([0, 0, 0, 0]).unwrap(), 0);
}

#[test]
fn codec_frame_len_is_little_endian() {
    assert_eq!(frame_len([0x01, 0, 0, 0]).unwrap(), 1);
    assert_eq!(frame_len([0x10, 0, 0, 0]).unwrap(), 16);
    assert_eq!(frame_len([0x01, 0x02, 0, 0]).unwrap(), 0x0201);
    assert_eq!(frame_len([0x01, 0x02, 0x03, 0]).unwrap(), 0x030201);
}

#[test]
fn codec_frame_len_accepts_the_maximum() {
    assert_eq!(
        frame_len(MAX_FRAME_LEN.to_le_bytes()).unwrap(),
        MAX_FRAME_LEN as usize
    );
    // 4 MiB is 0x0040_0000: little-endian, the 0x40 is the third byte.
    assert_eq!(frame_len([0, 0, 0x40, 0]).unwrap(), 4 * 1024 * 1024);
}

#[test]
fn codec_frame_len_refuses_one_over_the_maximum() {
    assert_err!(
        frame_len((MAX_FRAME_LEN + 1).to_le_bytes()),
        ProtocolError::FrameTooLarge { len: OVER_MAX }
    );
}

#[test]
fn codec_frame_len_refuses_u32_max() {
    assert_err!(
        frame_len(u32::MAX.to_le_bytes()),
        ProtocolError::FrameTooLarge { len: 0xffff_ffff }
    );
}

#[test]
fn codec_frame_len_refuses_a_length_with_a_high_byte() {
    // Read big-endian this would be 1; little-endian it is 33 554 432.
    assert_err!(
        frame_len([0, 0, 0, 0x02]),
        ProtocolError::FrameTooLarge { len: 0x0200_0000 }
    );
}

#[test]
fn codec_frame_len_refuses_the_text_of_an_http_request() {
    // DD-005: HTTP sent to the port reads as a length of about 540 MB.
    assert_err!(
        frame_len(*b"GET "),
        ProtocolError::FrameTooLarge { len: 0x2054_4547 }
    );
    assert_err!(frame_len(*b"POST"), ProtocolError::FrameTooLarge { .. });
}

/// A statement that makes a query body exactly `body_len` bytes.
fn query_with_body_len(body_len: u64) -> Request {
    let statement_len = usize::try_from(body_len - 5).unwrap();
    Request::Query {
        statement: "a".repeat(statement_len),
    }
}

#[test]
fn codec_to_frame_sends_a_body_of_exactly_the_maximum() {
    let query = query_with_body_len(MAX_FRAME_LEN.into());
    let frame = query.to_frame().expect("a body at the limit is allowed");
    assert_eq!(frame.len(), MAX_FRAME_LEN as usize + 4);
    assert_eq!(frame[..4], MAX_FRAME_LEN.to_le_bytes());
    assert_eq!(Request::from_payload(&frame[4..]).unwrap(), query);
}

#[test]
fn codec_to_frame_refuses_a_body_one_over_the_maximum() {
    let query = query_with_body_len(OVER_MAX);
    assert_err!(
        query.to_frame(),
        ProtocolError::FrameTooLarge { len: OVER_MAX }
    );
}

#[test]
fn codec_to_frame_refuses_a_statement_the_size_of_the_maximum_frame() {
    // The statement alone fills the frame; its type byte and length do not fit.
    let query = Request::Query {
        statement: "a".repeat(MAX_FRAME_LEN as usize),
    };
    assert_err!(query.to_frame(), ProtocolError::FrameTooLarge { .. });

    // write_to must fail before it writes anything.
    let mut out = Vec::new();
    assert_err!(
        query.write_to(&mut out),
        ProtocolError::FrameTooLarge { .. }
    );
    assert!(
        out.is_empty(),
        "wrote {} bytes of a refused frame",
        out.len()
    );
}

#[test]
fn codec_to_frame_refuses_a_response_over_the_maximum() {
    let huge = "a".repeat(MAX_FRAME_LEN as usize);
    assert_err!(
        Response::error(ErrorKind::Lex, huge.clone()).to_frame(),
        ProtocolError::FrameTooLarge { .. }
    );
    assert_err!(
        Response::Tokens(vec![token("Word", &huge, 1, 1)]).to_frame(),
        ProtocolError::FrameTooLarge { .. }
    );
}

// --- (e) read_payload ---

#[test]
fn codec_read_payload_at_a_clean_end_of_stream_is_none() {
    let mut reader: &[u8] = &[];
    assert_eq!(read_payload(&mut reader).unwrap(), None);

    let mut cursor = Cursor::new(Vec::<u8>::new());
    assert_eq!(read_payload(&mut cursor).unwrap(), None);
}

#[test]
fn codec_read_payload_ending_inside_the_header_is_truncated() {
    let header = [0x05, 0, 0, 0];
    for n in 1..=3 {
        let mut reader: &[u8] = &header[..n];
        let result = read_payload(&mut reader);
        assert!(
            matches!(result, Err(ProtocolError::Truncated)),
            "{n} header bytes: expected Truncated, got {result:?}"
        );
    }
}

#[test]
fn codec_read_payload_ending_inside_the_body_is_truncated() {
    // The header promises 5 bytes; only 2 follow.
    let mut reader: &[u8] = &[0x05, 0, 0, 0, 0xaa, 0xbb];
    assert_err!(read_payload(&mut reader), ProtocolError::Truncated);

    // The header promises 1 byte; none follow.
    let mut reader: &[u8] = &[0x01, 0, 0, 0];
    assert_err!(read_payload(&mut reader), ProtocolError::Truncated);
}

#[test]
fn codec_read_payload_of_every_prefix_of_every_frame() {
    let frames = request_fixtures()
        .into_iter()
        .map(|f| (f.name, f.frame))
        .chain(response_fixtures().into_iter().map(|f| (f.name, f.frame)));
    for (name, frame) in frames {
        for len in 0..=frame.len() {
            let mut reader: &[u8] = &frame[..len];
            let result = read_payload(&mut reader);
            match len {
                0 => assert!(
                    matches!(result, Ok(None)),
                    "{name}: empty: expected Ok(None), got {result:?}"
                ),
                n if n == frame.len() => assert_eq!(
                    result.unwrap().as_deref(),
                    Some(&frame[4..]),
                    "{name}: whole frame"
                ),
                n => assert!(
                    matches!(result, Err(ProtocolError::Truncated)),
                    "{name}: prefix of {n}: expected Truncated, got {result:?}"
                ),
            }
        }
    }
}

#[test]
fn codec_read_payload_returns_the_body_without_the_header() {
    let f = request_fixture("query");
    let mut reader: &[u8] = &f.frame;
    assert_eq!(read_payload(&mut reader).unwrap().unwrap(), f.body());
    assert!(reader.is_empty(), "the whole frame is consumed");
}

#[test]
fn codec_read_payload_of_an_empty_body_is_an_empty_payload() {
    let mut reader: &[u8] = &[0, 0, 0, 0];
    assert_eq!(read_payload(&mut reader).unwrap(), Some(Vec::new()));
}

#[test]
fn codec_an_empty_frame_body_is_truncated_as_a_message() {
    // "An empty body is truncated": there is no type byte.
    let mut reader: &[u8] = &[0, 0, 0, 0];
    assert_err!(Request::read_from(&mut reader), ProtocolError::Truncated);
    let mut reader: &[u8] = &[0, 0, 0, 0];
    assert_err!(Response::read_from(&mut reader), ProtocolError::Truncated);
}

#[test]
fn codec_read_payload_refuses_an_oversize_header_without_reading_the_body() {
    let mut stream = (MAX_FRAME_LEN + 1).to_le_bytes().to_vec();
    stream.extend_from_slice(&[0xaa; 64]);
    let mut cursor = Cursor::new(stream);
    assert_err!(
        read_payload(&mut cursor),
        ProtocolError::FrameTooLarge { len: OVER_MAX }
    );
    assert_eq!(cursor.position(), 4, "only the header may be read");
}

#[test]
fn codec_read_payload_refuses_u32_max_without_reading_the_body() {
    let mut stream = u32::MAX.to_le_bytes().to_vec();
    stream.extend_from_slice(&[0xaa; 64]);
    let mut cursor = Cursor::new(stream);
    assert_err!(
        read_payload(&mut cursor),
        ProtocolError::FrameTooLarge { len: 0xffff_ffff }
    );
    assert_eq!(cursor.position(), 4, "only the header may be read");
}

#[test]
fn codec_read_payload_refuses_an_http_request_without_reading_it() {
    let request = b"GET / HTTP/1.1\r\nHost: tdb\r\n\r\n";
    let mut cursor = Cursor::new(request.to_vec());
    assert_err!(
        read_payload(&mut cursor),
        ProtocolError::FrameTooLarge { len: 0x2054_4547 }
    );
    assert_eq!(cursor.position(), 4, "only the header may be read");
}

#[test]
fn codec_read_payload_accepts_a_header_of_the_maximum_but_needs_the_body() {
    // 16 MiB announced, nothing sent: truncated, not a hang or an allocation.
    let mut reader: &[u8] = &MAX_FRAME_LEN.to_le_bytes();
    let result = quickly(|| read_payload(&mut reader));
    assert!(
        matches!(result, Err(ProtocolError::Truncated)),
        "expected Truncated, got {result:?}"
    );

    // 16 MiB announced, ten bytes sent.
    let mut stream = MAX_FRAME_LEN.to_le_bytes().to_vec();
    stream.extend_from_slice(&[0xaa; 10]);
    let mut reader: &[u8] = &stream;
    let result = quickly(|| read_payload(&mut reader));
    assert!(
        matches!(result, Err(ProtocolError::Truncated)),
        "expected Truncated, got {result:?}"
    );
}

#[test]
fn codec_read_payload_reads_a_body_of_the_maximum() {
    let query = query_with_body_len(MAX_FRAME_LEN.into());
    let frame = query.to_frame().unwrap();
    let mut reader: &[u8] = &frame;
    let payload = read_payload(&mut reader).unwrap().unwrap();
    assert_eq!(payload.len(), MAX_FRAME_LEN as usize);
    assert!(reader.is_empty());
}

#[test]
fn codec_read_payload_reads_two_frames_back_to_back_then_none() {
    let hello = request_fixture("hello");
    let query = request_fixture("query");
    let stream = bytes(&[&hello.frame, &query.frame]);
    let mut reader: &[u8] = &stream;

    assert_eq!(read_payload(&mut reader).unwrap().unwrap(), hello.body());
    assert_eq!(read_payload(&mut reader).unwrap().unwrap(), query.body());
    assert_eq!(read_payload(&mut reader).unwrap(), None);
    assert_eq!(
        read_payload(&mut reader).unwrap(),
        None,
        "and it stays at the end"
    );
}

#[test]
fn codec_read_payload_reads_back_to_back_frames_from_a_cursor() {
    let frames: Vec<Vec<u8>> = response_fixtures().into_iter().map(|f| f.frame).collect();
    let mut cursor = Cursor::new(frames.concat());
    for frame in &frames {
        assert_eq!(read_payload(&mut cursor).unwrap().unwrap(), &frame[4..]);
    }
    assert_eq!(read_payload(&mut cursor).unwrap(), None);
}

/// A reader that hands out one byte per `read`, as a slow socket might.
struct OneByte<R>(R);

impl<R: Read> Read for OneByte<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(1);
        self.0.read(&mut buf[..n])
    }
}

#[test]
fn codec_read_payload_works_when_the_reader_returns_one_byte_at_a_time() {
    let hello = request_fixture("hello");
    let query = request_fixture("query_length_is_four_bytes_little_endian");
    let stream = bytes(&[&hello.frame, &query.frame]);
    let mut reader = OneByte(&stream[..]);

    assert_eq!(read_payload(&mut reader).unwrap().unwrap(), hello.body());
    assert_eq!(read_payload(&mut reader).unwrap().unwrap(), query.body());
    assert_eq!(read_payload(&mut reader).unwrap(), None);
}

#[test]
fn codec_read_from_works_when_the_reader_returns_one_byte_at_a_time() {
    let f = fixture("tokens");
    let stream = bytes(&[&f.frame, &f.frame]);
    let mut reader = OneByte(&stream[..]);
    assert_eq!(
        Response::read_from(&mut reader).unwrap().as_ref(),
        Some(&f.message)
    );
    assert_eq!(
        Response::read_from(&mut reader).unwrap().as_ref(),
        Some(&f.message)
    );
    assert_eq!(Response::read_from(&mut reader).unwrap(), None);
}

#[test]
fn codec_read_payload_one_byte_at_a_time_still_notices_a_truncated_frame() {
    let f = request_fixture("hello");
    let mut reader = OneByte(&f.frame[..f.frame.len() - 1]);
    assert_err!(read_payload(&mut reader), ProtocolError::Truncated);

    let mut reader = OneByte(&f.frame[..2]);
    assert_err!(read_payload(&mut reader), ProtocolError::Truncated);
}

/// A reader that serves `data`, then fails with `kind`.
struct ThenFail<'a> {
    data: &'a [u8],
    kind: io::ErrorKind,
}

impl Read for ThenFail<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.data.is_empty() {
            return Err(io::Error::new(self.kind, "the connection broke"));
        }
        self.data.read(buf)
    }
}

#[track_caller]
fn assert_io_kind<T: std::fmt::Debug>(result: Result<T, ProtocolError>, kind: io::ErrorKind) {
    match result {
        Err(ProtocolError::Io(e)) => assert_eq!(e.kind(), kind),
        other => panic!("expected Err(Io({kind:?})), got {other:?}"),
    }
}

#[test]
fn codec_read_payload_reports_a_reader_error_as_io() {
    let mut reader = ThenFail {
        data: &[],
        kind: io::ErrorKind::ConnectionReset,
    };
    assert_io_kind(read_payload(&mut reader), io::ErrorKind::ConnectionReset);
}

#[test]
fn codec_read_payload_reports_a_reader_error_inside_the_header_as_io() {
    let mut reader = ThenFail {
        data: &[0x05, 0],
        kind: io::ErrorKind::BrokenPipe,
    };
    assert_io_kind(read_payload(&mut reader), io::ErrorKind::BrokenPipe);
}

#[test]
fn codec_read_payload_reports_a_reader_error_inside_the_body_as_io() {
    let mut reader = ThenFail {
        data: &[0x05, 0, 0, 0, 0xaa, 0xbb],
        kind: io::ErrorKind::TimedOut,
    };
    assert_io_kind(read_payload(&mut reader), io::ErrorKind::TimedOut);

    // Right after the header, before any body byte.
    let mut reader = ThenFail {
        data: &[0x05, 0, 0, 0],
        kind: io::ErrorKind::ConnectionAborted,
    };
    assert_io_kind(read_payload(&mut reader), io::ErrorKind::ConnectionAborted);
}

#[test]
fn codec_read_from_reports_a_reader_error_as_io() {
    let mut reader = ThenFail {
        data: &[],
        kind: io::ErrorKind::ConnectionReset,
    };
    assert_io_kind(
        Request::read_from(&mut reader),
        io::ErrorKind::ConnectionReset,
    );
    let mut reader = ThenFail {
        data: &[],
        kind: io::ErrorKind::ConnectionReset,
    };
    assert_io_kind(
        Response::read_from(&mut reader),
        io::ErrorKind::ConnectionReset,
    );
}

/// A reader that fails with `Interrupted` before every successful read.
struct Interrupting<'a> {
    data: &'a [u8],
    interrupt_next: bool,
}

impl Read for Interrupting<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.interrupt_next = !self.interrupt_next;
        if self.interrupt_next {
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        let n = buf.len().min(1);
        self.data.read(&mut buf[..n])
    }
}

#[test]
fn codec_read_payload_retries_an_interrupted_read() {
    let f = request_fixture("hello");
    let mut reader = Interrupting {
        data: &f.frame,
        interrupt_next: false,
    };
    assert_eq!(read_payload(&mut reader).unwrap().unwrap(), f.body());
    assert_eq!(read_payload(&mut reader).unwrap(), None);
}

// --- (f) read_from and write_to ---

#[test]
fn codec_request_write_to_writes_the_frame() {
    for f in request_fixtures() {
        let mut out = Vec::new();
        f.message.write_to(&mut out).unwrap();
        assert_eq!(out, f.frame, "{}", f.name);
    }
}

#[test]
fn codec_response_write_to_writes_the_frame() {
    for f in response_fixtures() {
        let mut out = Vec::new();
        f.message.write_to(&mut out).unwrap();
        assert_eq!(out, f.frame, "{}", f.name);
    }
}

#[test]
fn codec_requests_round_trip_through_a_vec_and_a_cursor() {
    let requests: Vec<Request> = request_fixtures().into_iter().map(|f| f.message).collect();
    let mut wire = Vec::new();
    for request in &requests {
        request.write_to(&mut wire).unwrap();
    }
    let mut cursor = Cursor::new(wire);
    for request in &requests {
        assert_eq!(
            Request::read_from(&mut cursor).unwrap().as_ref(),
            Some(request)
        );
    }
    assert_eq!(Request::read_from(&mut cursor).unwrap(), None);
}

#[test]
fn codec_responses_round_trip_through_a_vec_and_a_cursor() {
    let responses: Vec<Response> = response_fixtures().into_iter().map(|f| f.message).collect();
    let mut wire = Vec::new();
    for response in &responses {
        response.write_to(&mut wire).unwrap();
    }
    let mut cursor = Cursor::new(wire);
    for response in &responses {
        assert_eq!(
            Response::read_from(&mut cursor).unwrap().as_ref(),
            Some(response)
        );
    }
    assert_eq!(Response::read_from(&mut cursor).unwrap(), None);
}

#[test]
fn codec_a_conversation_round_trips_in_order() {
    // The client writes; the server reads that, and writes back; and so on.
    let mut to_server = Vec::new();
    Request::Hello {
        version: VERSION,
        route: route("/graph/social"),
    }
    .write_to(&mut to_server)
    .unwrap();
    Request::Query {
        statement: "MATCH (n".to_owned(),
    }
    .write_to(&mut to_server)
    .unwrap();

    let mut server_in = &to_server[..];
    let hello = Request::read_from(&mut server_in).unwrap().unwrap();
    assert!(
        matches!(hello, Request::Hello { version: 1, .. }),
        "{hello:?}"
    );
    let query = Request::read_from(&mut server_in).unwrap().unwrap();
    assert_eq!(
        query,
        Request::Query {
            statement: "MATCH (n".to_owned()
        }
    );
    assert_eq!(Request::read_from(&mut server_in).unwrap(), None);

    let mut to_client = Vec::new();
    Response::HelloOk { version: VERSION }
        .write_to(&mut to_client)
        .unwrap();
    Response::error(ErrorKind::Lex, "unexpected end of input at 1:9")
        .write_to(&mut to_client)
        .unwrap();
    let mut client_in = &to_client[..];
    assert_eq!(
        Response::read_from(&mut client_in).unwrap(),
        Some(Response::HelloOk { version: VERSION })
    );
    assert_eq!(
        Response::read_from(&mut client_in).unwrap(),
        Some(Response::error(
            ErrorKind::Lex,
            "unexpected end of input at 1:9"
        ))
    );
    assert_eq!(Response::read_from(&mut client_in).unwrap(), None);
}

#[test]
fn codec_read_from_at_a_clean_end_of_stream_is_none() {
    let mut reader: &[u8] = &[];
    assert_eq!(Request::read_from(&mut reader).unwrap(), None);
    let mut reader: &[u8] = &[];
    assert_eq!(Response::read_from(&mut reader).unwrap(), None);
}

#[test]
fn codec_read_from_a_frame_cut_short_is_truncated() {
    let f = request_fixture("query");
    let mut reader: &[u8] = &f.frame[..f.frame.len() - 1];
    assert_err!(Request::read_from(&mut reader), ProtocolError::Truncated);
    let f = fixture("tokens");
    let mut reader: &[u8] = &f.frame[..f.frame.len() - 1];
    assert_err!(Response::read_from(&mut reader), ProtocolError::Truncated);
}

#[test]
fn codec_read_from_a_frame_with_an_unknown_tag_is_unknown_tag() {
    let mut reader: &[u8] = &[0x01, 0, 0, 0, 0x42];
    assert_err!(
        Request::read_from(&mut reader),
        ProtocolError::UnknownTag(0x42)
    );
    let mut reader: &[u8] = &[0x01, 0, 0, 0, 0x42];
    assert_err!(
        Response::read_from(&mut reader),
        ProtocolError::UnknownTag(0x42)
    );
}

#[test]
fn codec_read_from_the_wrong_direction_is_unknown_tag() {
    let hello = request_fixture("hello");
    let mut reader: &[u8] = &hello.frame;
    assert_err!(
        Response::read_from(&mut reader),
        ProtocolError::UnknownTag(0x01)
    );
    let tokens = fixture("tokens");
    let mut reader: &[u8] = &tokens.frame;
    assert_err!(
        Request::read_from(&mut reader),
        ProtocolError::UnknownTag(0x82)
    );
}

#[test]
fn codec_read_from_an_oversize_frame_is_frame_too_large() {
    let mut reader: &[u8] = &[0xff, 0xff, 0xff, 0xff, 1, 2, 3];
    assert_err!(
        Request::read_from(&mut reader),
        ProtocolError::FrameTooLarge { len: 0xffff_ffff }
    );
    let mut reader: &[u8] = b"GET / HTTP/1.1\r\n\r\n";
    assert_err!(
        Response::read_from(&mut reader),
        ProtocolError::FrameTooLarge { len: 0x2054_4547 }
    );
}

#[test]
fn codec_read_from_leaves_the_next_frame_unread_after_a_decode_error() {
    // One bad frame does not eat the one after it: framing and decoding are
    // separate, so a caller could carry on.
    let bad = [0x01, 0, 0, 0, 0x42];
    let good = request_fixture("query");
    let stream = bytes(&[&bad, &good.frame]);
    let mut reader: &[u8] = &stream;
    assert_err!(
        Request::read_from(&mut reader),
        ProtocolError::UnknownTag(0x42)
    );
    assert_eq!(
        Request::read_from(&mut reader).unwrap().as_ref(),
        Some(&good.message)
    );
}

/// A writer that fails the write, or only the flush.
struct BrokenWriter {
    fail_write: bool,
}

impl Write for BrokenWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.fail_write {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        } else {
            Ok(buf.len())
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::ConnectionReset))
    }
}

#[test]
fn codec_write_to_reports_a_write_error_as_io() {
    let request = request_fixture("query").message;
    let mut writer = BrokenWriter { fail_write: true };
    assert_io_kind(request.write_to(&mut writer), io::ErrorKind::BrokenPipe);

    let response = fixture("hello_ok").message;
    let mut writer = BrokenWriter { fail_write: true };
    assert_io_kind(response.write_to(&mut writer), io::ErrorKind::BrokenPipe);
}

#[test]
fn codec_write_to_reports_a_flush_error_as_io() {
    let request = request_fixture("query").message;
    let mut writer = BrokenWriter { fail_write: false };
    assert_io_kind(
        request.write_to(&mut writer),
        io::ErrorKind::ConnectionReset,
    );

    let response = fixture("hello_ok").message;
    let mut writer = BrokenWriter { fail_write: false };
    assert_io_kind(
        response.write_to(&mut writer),
        io::ErrorKind::ConnectionReset,
    );
}

// --- (g) WireToken, ErrorKind and ProtocolError ---

fn lex(model: Model, input: &str) -> Vec<Token> {
    Lexer::for_model(input, model)
        .tokenize()
        .expect("input lexes")
}

fn wire(tokens: &[Token]) -> Vec<WireToken> {
    tokens.iter().map(WireToken::from).collect()
}

#[test]
fn codec_wire_token_from_a_lexed_keyval_command() {
    let tokens = wire(&lex(Model::Keyval, "GET user:1"));
    assert_eq!(tokens[0], token("Keyword(GET)", "GET", 1, 1));
    assert_eq!(tokens[1], token("Word", "user:1", 1, 5));
    assert_eq!(tokens.last().unwrap().kind, "EndOfFile");
    assert_eq!(tokens.len(), 3);
}

#[test]
fn codec_wire_token_kinds_are_printed_as_token_type_prints_them() {
    for t in lex(Model::Graph, "MATCH (n:Person {age: 30}) RETURN n.name;") {
        let wire = WireToken::from(&t);
        assert_eq!(wire.kind, t.kind.to_string(), "kind of {t:?}");
        assert_eq!(wire.value, t.value, "value of {t:?}");
        assert_eq!(wire.line as usize, t.line, "line of {t:?}");
        assert_eq!(wire.column as usize, t.column, "column of {t:?}");
    }
}

#[test]
fn codec_wire_token_positions_are_one_based_across_lines() {
    let tokens = wire(&lex(Model::Keyval, "GET a\nSET b c"));
    let positions: Vec<(u32, u32)> = tokens.iter().map(|t| (t.line, t.column)).collect();
    assert_eq!(
        positions[..6],
        [(1, 1), (1, 5), (1, 6), (2, 1), (2, 5), (2, 7)],
        "{tokens:?}"
    );
}

#[test]
fn codec_wire_token_kind_depends_on_the_model() {
    // DD-004: MATCH is a keyword on a graph connection, a name on rdbms.
    let graph = wire(&lex(Model::Graph, "MATCH"));
    assert_eq!(graph[0].kind, "Keyword(MATCH)");
    let rdbms = wire(&lex(Model::Rdbms, "MATCH"));
    assert_eq!(rdbms[0].kind, "Identifier");
    assert_eq!(rdbms[0].value, "MATCH");
}

#[test]
fn codec_wire_token_saturates_a_position_that_does_not_fit() {
    let t = Token {
        kind: TokenType::Word,
        value: "x".to_owned(),
        line: usize::MAX,
        column: usize::MAX,
    };
    let wire = WireToken::from(&t);
    assert_eq!(wire.line, u32::MAX);
    assert_eq!(wire.column, u32::MAX);
}

#[test]
fn codec_wire_token_keeps_the_largest_position_that_fits() {
    let t = Token {
        kind: TokenType::Word,
        value: String::new(),
        line: u32::MAX as usize,
        column: 1,
    };
    let wire = WireToken::from(&t);
    assert_eq!(wire.line, u32::MAX);
    assert_eq!(wire.column, 1);
}

#[test]
fn codec_lexed_tokens_encode_to_the_tokens_fixture() {
    // Lexing "GET user:1" gives the same two tokens as the hand-built fixture
    // (the lexer's EndOfFile token follows them).
    let mut tokens = wire(&lex(Model::Keyval, "GET user:1"));
    assert_eq!(tokens.pop().unwrap().kind, "EndOfFile");
    let f = fixture("tokens");
    assert_response_layout(&Response::Tokens(tokens), &f.frame);
}

#[test]
fn codec_lexed_tokens_round_trip_for_every_model() {
    for (model, input) in [
        (
            Model::Keyval,
            "SET greeting \"hello world\"\nGET greeting\n",
        ),
        (Model::Rdbms, "SELECT id, name FROM users WHERE id = 1;"),
        (
            Model::Vector,
            "SELECT * FROM docs ORDER BY v <-> [1.0, 2.5] LIMIT 3;",
        ),
        (Model::Graph, "MATCH (a)-[:KNOWS]->(b) RETURN b;"),
    ] {
        let response = Response::Tokens(wire(&lex(model, input)));
        round_trip_response(&response);
    }
}

#[test]
fn codec_error_kind_codes_are_those_in_the_spec() {
    assert_eq!(ErrorKind::Protocol.code(), 1);
    assert_eq!(ErrorKind::Version.code(), 2);
    assert_eq!(ErrorKind::Route.code(), 3);
    assert_eq!(ErrorKind::Lex.code(), 4);
    assert_eq!(ErrorKind::TooLarge.code(), 5);
}

#[test]
fn codec_error_kind_code_round_trips() {
    for kind in [
        ErrorKind::Protocol,
        ErrorKind::Version,
        ErrorKind::Route,
        ErrorKind::Lex,
        ErrorKind::TooLarge,
    ] {
        assert_eq!(ErrorKind::from_code(kind.code()), Some(kind), "{kind:?}");
    }
    for code in 1..=5 {
        assert_eq!(ErrorKind::from_code(code).map(ErrorKind::code), Some(code));
    }
}

#[test]
fn codec_error_kind_from_code_refuses_codes_outside_1_to_5() {
    assert_eq!(ErrorKind::from_code(0), None);
    assert_eq!(ErrorKind::from_code(6), None);
    assert_eq!(ErrorKind::from_code(0xff), None);
    for code in (0..=u8::MAX).filter(|c| !(1..=5).contains(c)) {
        assert_eq!(ErrorKind::from_code(code), None, "code {code}");
    }
}

#[test]
fn codec_error_kind_of_a_route_error_is_route() {
    for e in [
        RouteError::MissingSlash,
        RouteError::MissingDatabase,
        RouteError::ExtraSegments,
        RouteError::UnknownModel("x".to_owned()),
        RouteError::EmptyDatabase,
        RouteError::DatabaseTooLong(65),
        RouteError::BadDatabaseChar('.'),
    ] {
        let err = ProtocolError::Route(e);
        assert_eq!(ErrorKind::of(&err), ErrorKind::Route, "{err:?}");
    }
}

#[test]
fn codec_error_kind_of_every_other_error_is_protocol() {
    for err in [
        ProtocolError::Io(io::Error::other("boom")),
        ProtocolError::Truncated,
        ProtocolError::FrameTooLarge { len: 99 },
        ProtocolError::UnknownTag(7),
        ProtocolError::BadMagic,
        ProtocolError::BadUtf8,
        ProtocolError::TrailingBytes(3),
        ProtocolError::UnknownErrorKind(9),
    ] {
        assert_eq!(ErrorKind::of(&err), ErrorKind::Protocol, "{err:?}");
    }
}

#[test]
fn codec_error_kind_of_a_failed_decode_follows_the_failure() {
    let truncated = Request::from_payload(&[]).unwrap_err();
    assert_eq!(ErrorKind::of(&truncated), ErrorKind::Protocol);
    let bad_magic = Request::from_payload(&bytes(&[&[0x01], b"NOPE", &[1, 0]])).unwrap_err();
    assert_eq!(ErrorKind::of(&bad_magic), ErrorKind::Protocol);
    let bad_route = Request::from_payload(&hello_body(1, b"/nope/x")).unwrap_err();
    assert_eq!(ErrorKind::of(&bad_route), ErrorKind::Route);
}

#[test]
fn codec_protocol_error_display_is_not_empty_for_any_variant() {
    for err in [
        ProtocolError::Io(io::Error::other("boom")),
        ProtocolError::Truncated,
        ProtocolError::FrameTooLarge { len: 99 },
        ProtocolError::UnknownTag(7),
        ProtocolError::BadMagic,
        ProtocolError::BadUtf8,
        ProtocolError::TrailingBytes(3),
        ProtocolError::UnknownErrorKind(9),
        ProtocolError::Route(RouteError::MissingSlash),
    ] {
        assert!(!err.to_string().is_empty(), "{err:?} has no message");
    }
}

#[test]
fn codec_protocol_error_display_says_what_it_carries() {
    let text = ProtocolError::Io(io::Error::other("boom")).to_string();
    assert!(text.contains("boom"), "{text}");
    let text = ProtocolError::FrameTooLarge { len: 123_456 }.to_string();
    assert!(text.contains("123456"), "{text}");
    let text = ProtocolError::TrailingBytes(37).to_string();
    assert!(text.contains("37"), "{text}");
    let text = ProtocolError::UnknownErrorKind(77).to_string();
    assert!(text.contains("77"), "{text}");
    let text = ProtocolError::Route(RouteError::UnknownModel("nosql".to_owned())).to_string();
    assert!(text.contains("nosql"), "{text}");
}

#[test]
fn codec_protocol_error_source_is_the_cause() {
    let io_err = ProtocolError::Io(io::Error::other("boom"));
    assert!(io_err.source().is_some());
    let route_err = ProtocolError::Route(RouteError::EmptyDatabase);
    assert!(route_err.source().is_some());
    for err in [
        ProtocolError::Truncated,
        ProtocolError::FrameTooLarge { len: 1 },
        ProtocolError::UnknownTag(1),
        ProtocolError::BadMagic,
        ProtocolError::BadUtf8,
        ProtocolError::TrailingBytes(1),
        ProtocolError::UnknownErrorKind(1),
    ] {
        assert!(err.source().is_none(), "{err:?} has no cause");
    }
}

#[test]
fn codec_protocol_error_converts_from_io_and_route_errors() {
    let err = ProtocolError::from(io::Error::from(io::ErrorKind::UnexpectedEof));
    assert_io_kind::<()>(Err(err), io::ErrorKind::UnexpectedEof);
    let err = ProtocolError::from(RouteError::ExtraSegments);
    assert_err!(
        Err::<(), _>(err),
        ProtocolError::Route(RouteError::ExtraSegments)
    );
}

// --- Limits for small frames, and the version read before the rest ---

#[test]
fn codec_frame_len_within_applies_the_given_limit() {
    assert_eq!(
        frame_len_within(MAX_HELLO_LEN.to_le_bytes(), MAX_HELLO_LEN).unwrap(),
        256
    );
    assert_err!(
        frame_len_within((MAX_HELLO_LEN + 1).to_le_bytes(), MAX_HELLO_LEN),
        ProtocolError::FrameTooLarge { len: 257 }
    );
    // A frame well under MAX_FRAME_LEN is still over a smaller limit.
    assert!(frame_len(1000u32.to_le_bytes()).is_ok());
    assert_err!(
        frame_len_within(1000u32.to_le_bytes(), MAX_HELLO_LEN),
        ProtocolError::FrameTooLarge { len: 1000 }
    );
}

#[test]
fn codec_a_real_hello_fits_in_the_hello_limit() {
    let longest = Route::new(tdb_lexer::Model::Vector, "x".repeat(64)).unwrap();
    let frame = Request::Hello {
        version: u16::MAX,
        route: longest,
    }
    .to_frame()
    .unwrap();
    let body = frame.len() - 4;
    assert!(body <= MAX_HELLO_LEN as usize, "{body} bytes");
}

#[test]
fn codec_hello_version_is_read_without_decoding_the_rest() {
    let hello = |version: u16, route: &[u8], tail: &[u8]| {
        let mut body = vec![0x01];
        body.extend_from_slice(&MAGIC);
        body.extend_from_slice(&version.to_le_bytes());
        body.extend_from_slice(&u32::try_from(route.len()).unwrap().to_le_bytes());
        body.extend_from_slice(route);
        body.extend_from_slice(tail);
        body
    };

    // A normal hello.
    assert_eq!(hello_version(&hello(1, b"/graph/a", b"")), Some(1));
    // A newer client's hello need not have a route this build accepts, nor end
    // where this build's hello ends; the version is still readable.
    assert_eq!(hello_version(&hello(2, b"/timeseries/x", b"")), Some(2));
    assert_eq!(
        hello_version(&hello(7, b"/graph/a", b"more fields")),
        Some(7)
    );
    assert_eq!(hello_version(&hello(3, &[0xff, 0xfe], b"")), Some(3));
    // Only the first seven bytes matter.
    let mut cut = hello(9, b"/graph/a", b"");
    cut.truncate(7);
    assert_eq!(hello_version(&cut), Some(9));

    // Not a hello: no version.
    assert_eq!(hello_version(&[]), None);
    assert_eq!(hello_version(&[0x01]), None);
    assert_eq!(hello_version(&cut[..6]), None);
    let mut bad_magic = hello(1, b"/graph/a", b"");
    bad_magic[1] = b'X';
    assert_eq!(hello_version(&bad_magic), None);
    let mut query = hello(1, b"/graph/a", b"");
    query[0] = 0x02;
    assert_eq!(hello_version(&query), None);
}
