//! `Endpoint`: the `tdb://host[:port]/<model>/<database>` URL of DD-005.
//! Hosts and ports, the default port, IPv6 in brackets, the round trip through
//! `Display`, and every way a URL can be wrong. Only the public API is used.

use std::error::Error;
use std::time::{Duration, Instant};

use tdb_lexer::Model;
use tdb_protocol::{DEFAULT_PORT, Endpoint, EndpointError, Route, RouteError, SCHEME};

fn parse(url: &str) -> Endpoint {
    match url.parse::<Endpoint>() {
        Ok(endpoint) => endpoint,
        Err(e) => panic!("{url:?} should parse, got {e:?}"),
    }
}

fn parse_err(url: &str) -> EndpointError {
    match url.parse::<Endpoint>() {
        Ok(endpoint) => panic!("{url:?} should not parse, got {endpoint:?}"),
        Err(e) => e,
    }
}

fn route(model: Model, database: &str) -> Route {
    Route::new(model, database).expect("valid route")
}

// --- Constants ---

#[test]
fn endpoint_default_port_is_7878() {
    assert_eq!(DEFAULT_PORT, 7878);
}

#[test]
fn endpoint_scheme_is_tdb() {
    assert_eq!(SCHEME, "tdb");
}

// --- Parsing ---

#[test]
fn endpoint_parses_host_port_model_and_database() {
    let endpoint = parse("tdb://127.0.0.1:7878/keyval/main");
    assert_eq!(endpoint.host(), "127.0.0.1");
    assert_eq!(endpoint.port(), 7878);
    assert_eq!(endpoint.route(), &route(Model::Keyval, "main"));
    assert_eq!(endpoint.route().model(), Model::Keyval);
    assert_eq!(endpoint.route().database(), "main");
}

#[test]
fn endpoint_parses_every_model() {
    for (model, name) in [
        (Model::Keyval, "keyval"),
        (Model::Rdbms, "rdbms"),
        (Model::Vector, "vector"),
        (Model::Graph, "graph"),
    ] {
        let endpoint = parse(&format!("tdb://localhost:1/{name}/db"));
        assert_eq!(endpoint.route().model(), model, "model {name}");
    }
}

#[test]
fn endpoint_port_defaults_to_7878_when_omitted() {
    let endpoint = parse("tdb://localhost/graph/social");
    assert_eq!(endpoint.host(), "localhost");
    assert_eq!(endpoint.port(), 7878);
    assert_eq!(endpoint.port(), DEFAULT_PORT);
    assert_eq!(endpoint.route(), &route(Model::Graph, "social"));
}

#[test]
fn endpoint_parses_an_explicit_port() {
    assert_eq!(parse("tdb://localhost:9000/graph/a").port(), 9000);
    assert_eq!(parse("tdb://localhost:1/graph/a").port(), 1);
}

#[test]
fn endpoint_parses_the_port_extremes() {
    assert_eq!(parse("tdb://localhost:0/graph/a").port(), 0);
    assert_eq!(parse("tdb://localhost:65535/graph/a").port(), 65535);
}

#[test]
fn endpoint_parses_an_ipv4_host() {
    let endpoint = parse("tdb://192.168.1.20:4000/rdbms/orders");
    assert_eq!(endpoint.host(), "192.168.1.20");
    assert_eq!(endpoint.port(), 4000);
    assert_eq!(endpoint.route(), &route(Model::Rdbms, "orders"));
}

#[test]
fn endpoint_parses_a_hostname() {
    let endpoint = parse("tdb://db-1.example.com:7000/vector/docs");
    assert_eq!(endpoint.host(), "db-1.example.com");
    assert_eq!(endpoint.port(), 7000);
}

#[test]
fn endpoint_keeps_the_case_of_the_host() {
    assert_eq!(parse("tdb://LocalHost/graph/a").host(), "LocalHost");
}

#[test]
fn endpoint_parses_a_bracketed_ipv6_host_without_the_brackets() {
    let endpoint = parse("tdb://[::1]:7878/vector/docs");
    assert_eq!(endpoint.host(), "::1");
    assert_eq!(endpoint.port(), 7878);
    assert_eq!(endpoint.route(), &route(Model::Vector, "docs"));
}

#[test]
fn endpoint_bracketed_ipv6_port_defaults_to_7878() {
    let endpoint = parse("tdb://[::1]/graph/a");
    assert_eq!(endpoint.host(), "::1");
    assert_eq!(endpoint.port(), DEFAULT_PORT);
}

#[test]
fn endpoint_parses_a_full_ipv6_address() {
    let endpoint = parse("tdb://[2001:db8:0:0:0:0:0:1]:4000/keyval/main");
    assert_eq!(endpoint.host(), "2001:db8:0:0:0:0:0:1");
    assert_eq!(endpoint.port(), 4000);
}

#[test]
fn endpoint_new_builds_the_same_endpoint_as_parsing() {
    let built = Endpoint::new("localhost", 7878, route(Model::Graph, "social"));
    assert_eq!(built, parse("tdb://localhost:7878/graph/social"));
    assert_eq!(built, parse("tdb://localhost/graph/social"));
}

#[test]
fn endpoint_equality_covers_host_port_and_route() {
    let endpoint = parse("tdb://a:1/graph/x");
    assert_eq!(endpoint, endpoint.clone());
    assert_ne!(endpoint, parse("tdb://b:1/graph/x"));
    assert_ne!(endpoint, parse("tdb://a:2/graph/x"));
    assert_ne!(endpoint, parse("tdb://a:1/graph/y"));
    assert_ne!(endpoint, parse("tdb://a:1/vector/x"));
}

// --- Display ---

#[test]
fn endpoint_display_always_writes_the_port() {
    assert_eq!(
        parse("tdb://localhost/graph/social").to_string(),
        "tdb://localhost:7878/graph/social"
    );
    assert_eq!(
        parse("tdb://127.0.0.1:9000/keyval/main").to_string(),
        "tdb://127.0.0.1:9000/keyval/main"
    );
}

#[test]
fn endpoint_display_puts_ipv6_in_brackets() {
    assert_eq!(
        parse("tdb://[::1]:7878/vector/docs").to_string(),
        "tdb://[::1]:7878/vector/docs"
    );
    assert_eq!(
        Endpoint::new("::1", 1, route(Model::Graph, "a")).to_string(),
        "tdb://[::1]:1/graph/a"
    );
}

#[test]
fn endpoint_display_round_trips() {
    for url in [
        "tdb://127.0.0.1:7878/keyval/main",
        "tdb://localhost/graph/social",
        "tdb://db-1.example.com:7000/rdbms/orders_2026",
        "tdb://[::1]:7878/vector/docs",
        "tdb://[::1]/vector/docs",
        "tdb://[2001:db8::1]:65535/graph/a-b_c",
        "tdb://h:0/keyval/x",
    ] {
        let endpoint = parse(url);
        let text = endpoint.to_string();
        assert_eq!(parse(&text), endpoint, "{url} -> {text}");
    }
}

#[test]
fn endpoint_display_round_trips_an_ipv6_endpoint_built_by_new() {
    let built = Endpoint::new("2001:db8::1", 4000, route(Model::Rdbms, "main"));
    let text = built.to_string();
    assert_eq!(text, "tdb://[2001:db8::1]:4000/rdbms/main");
    let reparsed = parse(&text);
    assert_eq!(reparsed, built);
    assert_eq!(reparsed.host(), "2001:db8::1");
}

// --- Wrong scheme ---

#[test]
fn endpoint_http_scheme_is_a_scheme_error() {
    assert_eq!(
        parse_err("http://localhost/graph/social"),
        EndpointError::Scheme
    );
}

#[test]
fn endpoint_single_slash_is_a_scheme_error() {
    assert_eq!(parse_err("tdb:/x"), EndpointError::Scheme);
    assert_eq!(parse_err("tdb:/localhost/graph/x"), EndpointError::Scheme);
}

#[test]
fn endpoint_no_scheme_is_a_scheme_error() {
    assert_eq!(
        parse_err("localhost:7878/graph/social"),
        EndpointError::Scheme
    );
    assert_eq!(parse_err("//localhost/graph/social"), EndpointError::Scheme);
    assert_eq!(parse_err("/graph/social"), EndpointError::Scheme);
    assert_eq!(parse_err(""), EndpointError::Scheme);
}

#[test]
fn endpoint_upper_case_scheme_is_a_scheme_error() {
    assert_eq!(
        parse_err("TDB://localhost/graph/social"),
        EndpointError::Scheme
    );
    assert_eq!(
        parse_err("Tdb://localhost/graph/social"),
        EndpointError::Scheme
    );
    assert_eq!(parse_err("TDB://"), EndpointError::Scheme);
}

#[test]
fn endpoint_a_longer_or_shorter_scheme_is_a_scheme_error() {
    assert_eq!(
        parse_err("tdbs://localhost/graph/social"),
        EndpointError::Scheme
    );
    assert_eq!(
        parse_err("xtdb://localhost/graph/social"),
        EndpointError::Scheme
    );
    assert_eq!(
        parse_err("td://localhost/graph/social"),
        EndpointError::Scheme
    );
    assert_eq!(parse_err("tdb"), EndpointError::Scheme);
    assert_eq!(parse_err("tdb:"), EndpointError::Scheme);
    assert_eq!(parse_err("tdb//localhost/graph/x"), EndpointError::Scheme);
}

// --- Hosts ---

#[test]
fn endpoint_empty_host_is_missing_host() {
    assert_eq!(parse_err("tdb:///graph/social"), EndpointError::MissingHost);
}

#[test]
fn endpoint_empty_host_with_a_port_is_missing_host() {
    assert_eq!(
        parse_err("tdb://:7878/graph/social"),
        EndpointError::MissingHost
    );
}

#[test]
fn endpoint_nothing_after_the_scheme_is_missing_host() {
    assert_eq!(parse_err("tdb://"), EndpointError::MissingHost);
}

#[test]
fn endpoint_empty_brackets_are_missing_host() {
    assert_eq!(parse_err("tdb://[]/graph/a"), EndpointError::MissingHost);
    assert_eq!(
        parse_err("tdb://[]:7878/graph/a"),
        EndpointError::MissingHost
    );
}

#[test]
fn endpoint_bare_ipv6_loopback_is_a_bad_host() {
    // Without brackets the colons are ambiguous with a port. The error should
    // be about the host, so its message can say to use `[::1]`.
    let err = parse_err("tdb://::1/graph/social");
    assert!(
        matches!(err, EndpointError::BadHost(_)),
        "expected BadHost, got {err:?}"
    );
}

#[test]
fn endpoint_bare_ipv6_address_is_a_bad_host() {
    let err = parse_err("tdb://2001:db8::1/graph/social");
    assert!(
        matches!(err, EndpointError::BadHost(_)),
        "expected BadHost, got {err:?}"
    );
}

#[test]
fn endpoint_bare_ipv6_address_with_a_port_is_a_bad_host() {
    for url in [
        "tdb://::1:7878/graph/social",
        "tdb://fe80::1:7878/graph/social",
        "tdb://2001:db8:0:0:0:0:0:1/graph/social",
    ] {
        let err = parse_err(url);
        assert!(
            matches!(err, EndpointError::BadHost(_)),
            "{url}: expected BadHost, got {err:?}"
        );
    }
}

#[test]
fn endpoint_unclosed_bracket_is_a_bad_host() {
    let err = parse_err("tdb://[::1/graph/social");
    assert!(
        matches!(err, EndpointError::BadHost(_)),
        "expected BadHost, got {err:?}"
    );
}

#[test]
fn endpoint_text_after_the_closing_bracket_is_a_bad_host() {
    for url in [
        "tdb://[::1]x/graph/a",
        "tdb://[::1]7878/graph/a",
        "tdb://[::1] /graph/a",
    ] {
        let err = parse_err(url);
        assert!(
            matches!(err, EndpointError::BadHost(_)),
            "{url}: expected BadHost, got {err:?}"
        );
    }
}

#[test]
fn endpoint_brackets_around_a_non_ipv6_host_are_a_bad_host() {
    for url in [
        "tdb://[localhost]/graph/a",
        "tdb://[127.0.0.1]:7878/graph/a",
    ] {
        let err = parse_err(url);
        assert!(
            matches!(err, EndpointError::BadHost(_)),
            "{url}: expected BadHost, got {err:?}"
        );
    }
}

#[test]
fn endpoint_user_info_is_refused_and_never_repeated() {
    for url in [
        "tdb://user@localhost/graph/a",
        "tdb://user:s3cr3t@localhost/graph/a",
        "tdb://user:s3cr3t@localhost:7878/graph/a",
        "tdb://:s3cr3t@[::1]:7878/graph/a",
    ] {
        let err = parse_err(url);
        assert!(
            matches!(err, EndpointError::UserInfo),
            "{url}: expected UserInfo, got {err:?}"
        );
        // Neither the error nor its message echoes the credentials.
        assert!(!format!("{err:?} {err}").contains("s3cr3t"), "{url}");
    }
}

#[test]
fn endpoint_port_is_digits_only() {
    // `u16::from_str` alone would take the first.
    for port in ["+80", "-1", " 80", "8_0", "0x50", "８０"] {
        let url = format!("tdb://localhost:{port}/graph/a");
        let err = parse_err(&url);
        assert!(
            matches!(err, EndpointError::BadPort(_)),
            "{url}: expected BadPort, got {err:?}"
        );
    }
    assert_eq!(parse("tdb://localhost:00080/graph/a").port(), 80);
}

#[test]
fn endpoint_query_and_fragment_characters_in_the_host_are_a_bad_host() {
    for url in [
        "tdb://host?x=1/graph/a",
        "tdb://host#frag/graph/a",
        "tdb://ho st/graph/a",
    ] {
        let err = parse_err(url);
        assert!(
            matches!(err, EndpointError::BadHost(_) | EndpointError::BadPort(_)),
            "{url}: got {err:?}"
        );
    }
}

#[test]
fn endpoint_a_space_in_the_host_is_a_bad_host() {
    let err = parse_err("tdb://local host/graph/a");
    assert!(
        matches!(err, EndpointError::BadHost(_)),
        "expected BadHost, got {err:?}"
    );
}

// --- Ports ---

#[test]
fn endpoint_a_non_numeric_port_is_a_bad_port() {
    assert_eq!(
        parse_err("tdb://localhost:http/graph/a"),
        EndpointError::BadPort("http".to_owned())
    );
    assert_eq!(
        parse_err("tdb://localhost:78x8/graph/a"),
        EndpointError::BadPort("78x8".to_owned())
    );
}

#[test]
fn endpoint_a_port_above_65535_is_a_bad_port() {
    assert_eq!(
        parse_err("tdb://localhost:65536/graph/a"),
        EndpointError::BadPort("65536".to_owned())
    );
}

#[test]
fn endpoint_a_port_that_overflows_every_integer_is_a_bad_port() {
    assert_eq!(
        parse_err("tdb://localhost:99999999999999999999999999/graph/a"),
        EndpointError::BadPort("99999999999999999999999999".to_owned())
    );
    assert_eq!(
        parse_err("tdb://localhost:4294967296/graph/a"),
        EndpointError::BadPort("4294967296".to_owned())
    );
}

#[test]
fn endpoint_an_empty_port_is_a_bad_port() {
    assert_eq!(
        parse_err("tdb://localhost:/graph/a"),
        EndpointError::BadPort(String::new())
    );
}

#[test]
fn endpoint_an_empty_port_after_ipv6_brackets_is_a_bad_port() {
    assert_eq!(
        parse_err("tdb://[::1]:/graph/a"),
        EndpointError::BadPort(String::new())
    );
}

#[test]
fn endpoint_a_negative_port_is_a_bad_port() {
    assert_eq!(
        parse_err("tdb://localhost:-1/graph/a"),
        EndpointError::BadPort("-1".to_owned())
    );
}

#[test]
fn endpoint_a_port_with_a_space_is_a_bad_port() {
    assert_eq!(
        parse_err("tdb://localhost: 80/graph/a"),
        EndpointError::BadPort(" 80".to_owned())
    );
    assert_eq!(
        parse_err("tdb://localhost:80 /graph/a"),
        EndpointError::BadPort("80 ".to_owned())
    );
}

#[test]
fn endpoint_a_decimal_port_is_a_bad_port() {
    assert_eq!(
        parse_err("tdb://localhost:78.78/graph/a"),
        EndpointError::BadPort("78.78".to_owned())
    );
}

#[test]
fn endpoint_a_port_after_ipv6_brackets_is_checked_too() {
    assert_eq!(
        parse_err("tdb://[::1]:65536/graph/a"),
        EndpointError::BadPort("65536".to_owned())
    );
    assert_eq!(
        parse_err("tdb://[::1]:abc/graph/a"),
        EndpointError::BadPort("abc".to_owned())
    );
}

// --- Missing or bad path ---

#[test]
fn endpoint_no_path_is_a_missing_slash_route_error() {
    assert_eq!(
        parse_err("tdb://host:1"),
        EndpointError::Route(RouteError::MissingSlash)
    );
    assert_eq!(
        parse_err("tdb://host"),
        EndpointError::Route(RouteError::MissingSlash)
    );
    assert_eq!(
        parse_err("tdb://[::1]:7878"),
        EndpointError::Route(RouteError::MissingSlash)
    );
}

#[test]
fn endpoint_a_lone_slash_is_a_missing_database_route_error() {
    assert_eq!(
        parse_err("tdb://host/"),
        EndpointError::Route(RouteError::MissingDatabase)
    );
}

#[test]
fn endpoint_a_model_without_a_database_is_a_route_error() {
    assert_eq!(
        parse_err("tdb://host:1/graph"),
        EndpointError::Route(RouteError::MissingDatabase)
    );
    assert_eq!(
        parse_err("tdb://host:1/graph/"),
        EndpointError::Route(RouteError::EmptyDatabase)
    );
}

#[test]
fn endpoint_an_unknown_model_is_a_route_error() {
    assert_eq!(
        parse_err("tdb://host/nosql/x"),
        EndpointError::Route(RouteError::UnknownModel("nosql".to_owned()))
    );
    assert_eq!(
        parse_err("tdb://host/GRAPH/x"),
        EndpointError::Route(RouteError::UnknownModel("GRAPH".to_owned()))
    );
}

#[test]
fn endpoint_extra_path_segments_are_a_route_error() {
    assert_eq!(
        parse_err("tdb://host/graph/a/b"),
        EndpointError::Route(RouteError::ExtraSegments)
    );
}

#[test]
fn endpoint_a_bad_database_name_is_a_route_error() {
    assert_eq!(
        parse_err("tdb://host/graph/a.b"),
        EndpointError::Route(RouteError::BadDatabaseChar('.'))
    );
    assert_eq!(
        parse_err("tdb://host/graph/.."),
        EndpointError::Route(RouteError::BadDatabaseChar('.'))
    );
    assert_eq!(
        parse_err("tdb://host/graph/a b"),
        EndpointError::Route(RouteError::BadDatabaseChar(' '))
    );
}

#[test]
fn endpoint_a_query_string_is_a_route_error() {
    assert_eq!(
        parse_err("tdb://host/graph/social?x=1"),
        EndpointError::Route(RouteError::BadDatabaseChar('?'))
    );
}

#[test]
fn endpoint_a_fragment_is_a_route_error() {
    assert_eq!(
        parse_err("tdb://host/graph/social#top"),
        EndpointError::Route(RouteError::BadDatabaseChar('#'))
    );
}

#[test]
fn endpoint_a_database_name_over_64_bytes_is_a_route_error() {
    let url = format!("tdb://host/graph/{}", "a".repeat(65));
    assert_eq!(
        parse_err(&url),
        EndpointError::Route(RouteError::DatabaseTooLong(65))
    );
}

#[test]
fn endpoint_a_database_name_of_64_bytes_is_accepted() {
    let name = "a".repeat(64);
    let endpoint = parse(&format!("tdb://host/graph/{name}"));
    assert_eq!(endpoint.route().database(), name);
}

#[test]
fn endpoint_a_million_slashes_is_an_error_and_quick() {
    let url = format!("tdb://host{}", "/".repeat(1_000_000));
    let start = Instant::now();
    let err = parse_err(&url);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "took {:?}",
        start.elapsed()
    );
    assert_eq!(err, EndpointError::Route(RouteError::ExtraSegments));
}

#[test]
fn endpoint_a_million_byte_model_is_echoed_in_at_most_32_characters() {
    let url = format!("tdb://host/{}/db", "m".repeat(1_000_000));
    assert_eq!(
        parse_err(&url),
        EndpointError::Route(RouteError::UnknownModel("m".repeat(32)))
    );
}

// --- Errors as values ---

#[test]
fn endpoint_error_display_is_not_empty_for_any_variant() {
    let errors = [
        EndpointError::Scheme,
        EndpointError::MissingHost,
        EndpointError::BadHost("::1".to_owned()),
        EndpointError::BadPort("99999".to_owned()),
        EndpointError::Route(RouteError::MissingSlash),
    ];
    for err in errors {
        assert!(!err.to_string().is_empty(), "{err:?} has no message");
    }
}

#[test]
fn endpoint_error_display_names_what_is_wrong() {
    assert!(EndpointError::Scheme.to_string().contains("tdb://"));
    assert!(
        EndpointError::BadPort("99999".to_owned())
            .to_string()
            .contains("99999")
    );
    assert!(
        EndpointError::BadHost("::1".to_owned())
            .to_string()
            .contains("::1")
    );
}

#[test]
fn endpoint_error_converts_from_a_route_error() {
    let err = EndpointError::from(RouteError::ExtraSegments);
    assert_eq!(err, EndpointError::Route(RouteError::ExtraSegments));
}

#[test]
fn endpoint_error_source_is_the_route_error() {
    let err = EndpointError::Route(RouteError::EmptyDatabase);
    let source = err.source().expect("a route error has a source");
    assert_eq!(source.to_string(), RouteError::EmptyDatabase.to_string());
    assert!(err.to_string().contains(&source.to_string()));
}

#[test]
fn endpoint_error_source_is_none_for_the_other_variants() {
    assert!(EndpointError::Scheme.source().is_none());
    assert!(EndpointError::MissingHost.source().is_none());
    assert!(EndpointError::BadHost("x".to_owned()).source().is_none());
    assert!(EndpointError::BadPort("x".to_owned()).source().is_none());
}
