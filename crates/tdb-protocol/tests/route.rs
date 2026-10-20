//! `Route`: the `/<model>/<database>` path of DD-004 ("Route parsing") and
//! DD-005. Parsing, the database-name rules, the shapes that are rejected, and
//! hostile input. Only the public API is used.

use std::error::Error;
use std::time::{Duration, Instant};

use tdb_lexer::Model;
use tdb_protocol::{MAX_DATABASE_LEN, Route, RouteError, model_name};

const ALL_MODELS: [(Model, &str); 4] = [
    (Model::Keyval, "keyval"),
    (Model::Rdbms, "rdbms"),
    (Model::Vector, "vector"),
    (Model::Graph, "graph"),
];

/// Nothing in this file may take anywhere near this long, even in a debug
/// build on a busy machine.
const QUICKLY: Duration = Duration::from_secs(5);

fn parse(path: &str) -> Route {
    match path.parse::<Route>() {
        Ok(route) => route,
        Err(e) => panic!("{path:?} should parse, got {e:?}"),
    }
}

fn parse_err(path: &str) -> RouteError {
    match path.parse::<Route>() {
        Ok(route) => panic!("{path:?} should not parse, got {route:?}"),
        Err(e) => e,
    }
}

/// A database name of exactly `len` bytes, all allowed.
fn name_of(len: usize) -> String {
    "a".repeat(len)
}

// --- Parsing ---

#[test]
fn route_parses_every_model() {
    for (model, name) in ALL_MODELS {
        let route = parse(&format!("/{name}/main"));
        assert_eq!(route.model(), model, "model of /{name}/main");
        assert_eq!(route.database(), "main");
    }
}

#[test]
fn route_parses_through_from_str() {
    use std::str::FromStr;
    let route = Route::from_str("/graph/social").unwrap();
    assert_eq!(route, "/graph/social".parse::<Route>().unwrap());
    assert_eq!(route.model(), Model::Graph);
    assert_eq!(route.database(), "social");
}

#[test]
fn route_display_is_the_path_form() {
    let route = Route::new(Model::Graph, "social").unwrap();
    assert_eq!(route.to_string(), "/graph/social");
}

#[test]
fn route_display_round_trips_every_model() {
    for (model, name) in ALL_MODELS {
        for database in ["main", "a", "my_db-2", "UPPER", "0", "-", "_"] {
            let route = Route::new(model, database).unwrap();
            let text = route.to_string();
            assert_eq!(text, format!("/{name}/{database}"));
            assert_eq!(parse(&text), route, "round trip of {text:?}");
        }
    }
}

#[test]
fn route_display_round_trips_the_longest_name() {
    let route = Route::new(Model::Vector, name_of(MAX_DATABASE_LEN)).unwrap();
    assert_eq!(parse(&route.to_string()), route);
}

#[test]
fn route_model_name_is_the_path_segment() {
    for (model, name) in ALL_MODELS {
        assert_eq!(model_name(model), name);
    }
}

// --- Accessors ---

#[test]
fn route_accessors_return_what_was_given() {
    let route = Route::new(Model::Rdbms, "orders_2026").unwrap();
    assert_eq!(route.model(), Model::Rdbms);
    assert_eq!(route.database(), "orders_2026");
}

#[test]
fn route_new_accepts_a_string_or_a_str() {
    let from_str = Route::new(Model::Keyval, "main").unwrap();
    let from_string = Route::new(Model::Keyval, String::from("main")).unwrap();
    assert_eq!(from_str, from_string);
}

#[test]
fn route_equality_covers_model_and_database() {
    let route = Route::new(Model::Graph, "a").unwrap();
    assert_eq!(route, route.clone());
    assert_ne!(route, Route::new(Model::Vector, "a").unwrap());
    assert_ne!(route, Route::new(Model::Graph, "b").unwrap());
}

#[test]
fn route_max_database_len_is_64() {
    assert_eq!(MAX_DATABASE_LEN, 64);
}

// --- Database names ---

#[test]
fn route_database_accepts_every_allowed_character() {
    let all: String = ('a'..='z')
        .chain('A'..='Z')
        .chain('0'..='9')
        .chain(['_', '-'])
        .collect();
    assert_eq!(parse(&format!("/keyval/{all}")).database(), all);
}

#[test]
fn route_database_of_one_byte_is_accepted() {
    assert_eq!(parse("/graph/x").database(), "x");
}

#[test]
fn route_database_of_64_bytes_is_accepted() {
    let name = name_of(64);
    assert_eq!(parse(&format!("/graph/{name}")).database(), name);
}

#[test]
fn route_database_of_65_bytes_is_too_long() {
    assert_eq!(
        parse_err(&format!("/graph/{}", name_of(65))),
        RouteError::DatabaseTooLong(65)
    );
}

#[test]
fn route_database_too_long_reports_the_length() {
    assert_eq!(
        parse_err(&format!("/graph/{}", name_of(1000))),
        RouteError::DatabaseTooLong(1000)
    );
}

#[test]
fn route_database_empty_is_empty_database() {
    assert_eq!(parse_err("/graph/"), RouteError::EmptyDatabase);
}

#[test]
fn route_database_dot_is_a_bad_char() {
    assert_eq!(parse_err("/graph/."), RouteError::BadDatabaseChar('.'));
}

#[test]
fn route_database_dot_dot_is_a_bad_char() {
    assert_eq!(parse_err("/graph/.."), RouteError::BadDatabaseChar('.'));
}

#[test]
fn route_database_dot_inside_a_name_is_a_bad_char() {
    assert_eq!(parse_err("/graph/a.b"), RouteError::BadDatabaseChar('.'));
    assert_eq!(
        parse_err("/graph/.hidden"),
        RouteError::BadDatabaseChar('.')
    );
    assert_eq!(parse_err("/graph/db."), RouteError::BadDatabaseChar('.'));
}

#[test]
fn route_database_slash_is_a_bad_char_through_new() {
    // Parsing splits on `/`, so a slash can only reach the name through `new`.
    assert_eq!(
        Route::new(Model::Graph, "a/b").unwrap_err(),
        RouteError::BadDatabaseChar('/')
    );
    assert_eq!(
        Route::new(Model::Graph, "/").unwrap_err(),
        RouteError::BadDatabaseChar('/')
    );
    assert_eq!(
        Route::new(Model::Graph, "../etc").unwrap_err(),
        RouteError::BadDatabaseChar('.')
    );
}

#[test]
fn route_database_backslash_is_a_bad_char() {
    assert_eq!(parse_err("/graph/a\\b"), RouteError::BadDatabaseChar('\\'));
}

#[test]
fn route_database_space_is_a_bad_char() {
    assert_eq!(parse_err("/graph/a b"), RouteError::BadDatabaseChar(' '));
    assert_eq!(parse_err("/graph/ "), RouteError::BadDatabaseChar(' '));
}

#[test]
fn route_database_control_characters_are_bad_chars() {
    assert_eq!(parse_err("/graph/a\0b"), RouteError::BadDatabaseChar('\0'));
    assert_eq!(parse_err("/graph/a\nb"), RouteError::BadDatabaseChar('\n'));
    assert_eq!(parse_err("/graph/a\tb"), RouteError::BadDatabaseChar('\t'));
    assert_eq!(parse_err("/graph/a\rb"), RouteError::BadDatabaseChar('\r'));
}

#[test]
fn route_database_non_ascii_is_a_bad_char() {
    assert_eq!(
        parse_err("/graph/caf\u{e9}"),
        RouteError::BadDatabaseChar('\u{e9}')
    );
    assert_eq!(
        parse_err("/graph/\u{65e5}\u{672c}"),
        RouteError::BadDatabaseChar('\u{65e5}')
    );
    assert_eq!(
        parse_err("/graph/db\u{1f980}"),
        RouteError::BadDatabaseChar('\u{1f980}')
    );
}

#[test]
fn route_database_non_ascii_digits_and_letters_are_bad_chars() {
    // Alphanumeric to Unicode, but not to `[A-Za-z0-9]`.
    assert_eq!(
        parse_err("/graph/\u{663}"),
        RouteError::BadDatabaseChar('\u{663}')
    );
    assert_eq!(
        parse_err("/graph/\u{ff21}"),
        RouteError::BadDatabaseChar('\u{ff21}')
    );
    assert_eq!(
        parse_err("/graph/\u{3b1}"),
        RouteError::BadDatabaseChar('\u{3b1}')
    );
}

#[test]
fn route_database_query_string_is_a_bad_char() {
    assert_eq!(
        parse_err("/graph/social?x=1"),
        RouteError::BadDatabaseChar('?')
    );
    assert_eq!(parse_err("/graph/?x=1"), RouteError::BadDatabaseChar('?'));
}

#[test]
fn route_database_other_punctuation_is_a_bad_char() {
    for bad in [
        '#', '%', '&', '=', '+', ':', ';', '@', '*', '~', '!', '$', '\'', '"', '<', '>', '|', '(',
        ')', '[', ']', '{', '}', ',',
    ] {
        assert_eq!(
            parse_err(&format!("/graph/a{bad}b")),
            RouteError::BadDatabaseChar(bad),
            "database name containing {bad:?}"
        );
    }
}

#[test]
fn route_database_reports_the_first_bad_char() {
    assert_eq!(parse_err("/graph/a.b?c"), RouteError::BadDatabaseChar('.'));
    assert_eq!(parse_err("/graph/ok?x.y"), RouteError::BadDatabaseChar('?'));
}

// --- Wrong shapes ---

#[test]
fn route_without_a_leading_slash_is_missing_slash() {
    assert_eq!(parse_err("graph/social"), RouteError::MissingSlash);
    assert_eq!(parse_err("graph"), RouteError::MissingSlash);
    assert_eq!(parse_err(" /graph/social"), RouteError::MissingSlash);
}

#[test]
fn route_empty_path_is_missing_slash() {
    assert_eq!(parse_err(""), RouteError::MissingSlash);
}

#[test]
fn route_a_lone_slash_is_missing_database() {
    assert_eq!(parse_err("/"), RouteError::MissingDatabase);
}

#[test]
fn route_a_model_alone_is_missing_database() {
    assert_eq!(parse_err("/graph"), RouteError::MissingDatabase);
    assert_eq!(parse_err("/keyval"), RouteError::MissingDatabase);
}

#[test]
fn route_a_model_and_a_trailing_slash_is_empty_database() {
    assert_eq!(parse_err("/graph/"), RouteError::EmptyDatabase);
    assert_eq!(parse_err("/rdbms/"), RouteError::EmptyDatabase);
}

#[test]
fn route_a_third_segment_is_extra_segments() {
    assert_eq!(parse_err("/graph/a/b"), RouteError::ExtraSegments);
    assert_eq!(parse_err("/graph/a/b/c/d"), RouteError::ExtraSegments);
}

#[test]
fn route_a_trailing_slash_after_the_database_is_extra_segments() {
    assert_eq!(parse_err("/graph/a/"), RouteError::ExtraSegments);
}

#[test]
fn route_an_empty_database_with_more_after_it_is_extra_segments() {
    assert_eq!(parse_err("/graph//"), RouteError::ExtraSegments);
    assert_eq!(parse_err("/graph//x"), RouteError::ExtraSegments);
}

#[test]
fn route_dot_dot_as_a_segment_is_extra_segments() {
    assert_eq!(parse_err("/graph/../x"), RouteError::ExtraSegments);
}

#[test]
fn route_an_empty_model_is_unknown_model() {
    assert_eq!(parse_err("//x"), RouteError::UnknownModel(String::new()));
}

#[test]
fn route_an_unknown_model_is_unknown_model() {
    assert_eq!(
        parse_err("/nosql/x"),
        RouteError::UnknownModel("nosql".to_owned())
    );
    assert_eq!(
        parse_err("/graphs/x"),
        RouteError::UnknownModel("graphs".to_owned())
    );
    assert_eq!(
        parse_err("/gra/x"),
        RouteError::UnknownModel("gra".to_owned())
    );
}

#[test]
fn route_model_names_are_lower_case_only() {
    for name in [
        "KEYVAL", "Keyval", "keyVal", "RDBMS", "Rdbms", "VECTOR", "Vector", "GRAPH", "Graph",
        "gRaPh",
    ] {
        assert_eq!(
            parse_err(&format!("/{name}/x")),
            RouteError::UnknownModel(name.to_owned()),
            "model {name:?}"
        );
    }
}

#[test]
fn route_model_names_are_not_trimmed() {
    assert_eq!(
        parse_err("/ graph/x"),
        RouteError::UnknownModel(" graph".to_owned())
    );
    assert_eq!(
        parse_err("/graph /x"),
        RouteError::UnknownModel("graph ".to_owned())
    );
}

// --- Route::new ---

#[test]
fn route_new_rejects_an_empty_database() {
    assert_eq!(
        Route::new(Model::Graph, "").unwrap_err(),
        RouteError::EmptyDatabase
    );
}

#[test]
fn route_new_rejects_a_database_of_65_bytes() {
    assert_eq!(
        Route::new(Model::Graph, name_of(65)).unwrap_err(),
        RouteError::DatabaseTooLong(65)
    );
}

#[test]
fn route_new_accepts_a_database_of_64_bytes() {
    let route = Route::new(Model::Graph, name_of(64)).unwrap();
    assert_eq!(route.database().len(), 64);
}

#[test]
fn route_new_rejects_bad_characters() {
    assert_eq!(
        Route::new(Model::Graph, "a b").unwrap_err(),
        RouteError::BadDatabaseChar(' ')
    );
    assert_eq!(
        Route::new(Model::Graph, "a\0").unwrap_err(),
        RouteError::BadDatabaseChar('\0')
    );
    assert_eq!(
        Route::new(Model::Graph, "caf\u{e9}").unwrap_err(),
        RouteError::BadDatabaseChar('\u{e9}')
    );
    assert_eq!(
        Route::new(Model::Graph, "x?y=1").unwrap_err(),
        RouteError::BadDatabaseChar('?')
    );
}

#[test]
fn route_new_validates_like_parsing() {
    let names = [
        "".to_owned(),
        "a".to_owned(),
        "db-1_x".to_owned(),
        ".".to_owned(),
        "..".to_owned(),
        "a b".to_owned(),
        "a\0b".to_owned(),
        "caf\u{e9}".to_owned(),
        "x?y=1".to_owned(),
        name_of(64),
        name_of(65),
        name_of(10_000),
    ];
    for (model, segment) in ALL_MODELS {
        for name in &names {
            let built = Route::new(model, name.as_str());
            let parsed = format!("/{segment}/{name}").parse::<Route>();
            assert_eq!(built, parsed, "{segment} / {name:?}");
        }
    }
}

// --- Hostile input ---

#[test]
fn route_unknown_model_echoes_at_most_32_characters() {
    let hostile = "x".repeat(10_000);
    let err = parse_err(&format!("/{hostile}/db"));
    assert_eq!(err, RouteError::UnknownModel("x".repeat(32)));
}

#[test]
fn route_unknown_model_keeps_a_32_character_name_whole() {
    let name = "y".repeat(32);
    assert_eq!(
        parse_err(&format!("/{name}/db")),
        RouteError::UnknownModel(name)
    );
}

#[test]
fn route_unknown_model_cuts_a_33_character_name_to_32() {
    let name = "y".repeat(33);
    assert_eq!(
        parse_err(&format!("/{name}/db")),
        RouteError::UnknownModel("y".repeat(32))
    );
}

#[test]
fn route_unknown_model_counts_characters_not_bytes() {
    let hostile = "\u{e9}".repeat(1000);
    match parse_err(&format!("/{hostile}/db")) {
        RouteError::UnknownModel(echo) => {
            assert_eq!(echo, "\u{e9}".repeat(32));
            assert_eq!(echo.chars().count(), 32);
        }
        other => panic!("expected UnknownModel, got {other:?}"),
    }
}

#[test]
fn route_unknown_model_error_message_stays_small() {
    let hostile = "z".repeat(100_000);
    let err = parse_err(&format!("/{hostile}/db"));
    assert!(
        err.to_string().len() < 200,
        "message is {} bytes",
        err.to_string().len()
    );
}

#[test]
fn route_a_million_slashes_is_an_error_and_quick() {
    let path = "/".repeat(1_000_000);
    let start = Instant::now();
    let err = parse_err(&path);
    assert!(start.elapsed() < QUICKLY, "took {:?}", start.elapsed());
    assert_eq!(err, RouteError::ExtraSegments);
}

#[test]
fn route_a_million_slashes_after_a_model_is_an_error_and_quick() {
    let path = format!("/graph{}", "/".repeat(1_000_000));
    let start = Instant::now();
    let err = parse_err(&path);
    assert!(start.elapsed() < QUICKLY, "took {:?}", start.elapsed());
    assert_eq!(err, RouteError::ExtraSegments);
}

#[test]
fn route_a_million_byte_model_is_an_error_and_quick() {
    let path = format!("/{}/db", "m".repeat(1_000_000));
    let start = Instant::now();
    let err = parse_err(&path);
    assert!(start.elapsed() < QUICKLY, "took {:?}", start.elapsed());
    assert_eq!(err, RouteError::UnknownModel("m".repeat(32)));
}

#[test]
fn route_a_million_byte_database_is_too_long_and_quick() {
    let path = format!("/graph/{}", "d".repeat(1_000_000));
    let start = Instant::now();
    let err = parse_err(&path);
    assert!(start.elapsed() < QUICKLY, "took {:?}", start.elapsed());
    assert_eq!(err, RouteError::DatabaseTooLong(1_000_000));
}

#[test]
fn route_a_million_non_ascii_bytes_is_an_error_and_quick() {
    let path = format!("/graph/{}", "\u{e9}".repeat(500_000));
    let start = Instant::now();
    let result = path.parse::<Route>();
    assert!(start.elapsed() < QUICKLY, "took {:?}", start.elapsed());
    assert!(result.is_err(), "got {result:?}");
}

// --- Errors as values ---

#[test]
fn route_error_display_is_not_empty_for_any_variant() {
    let errors = [
        RouteError::MissingSlash,
        RouteError::MissingDatabase,
        RouteError::ExtraSegments,
        RouteError::UnknownModel("nope".to_owned()),
        RouteError::EmptyDatabase,
        RouteError::DatabaseTooLong(65),
        RouteError::BadDatabaseChar('.'),
    ];
    for err in errors {
        assert!(!err.to_string().is_empty(), "{err:?} has no message");
    }
}

#[test]
fn route_error_display_names_what_is_wrong() {
    assert!(
        RouteError::UnknownModel("nope".to_owned())
            .to_string()
            .contains("nope")
    );
    assert!(RouteError::DatabaseTooLong(65).to_string().contains("65"));
    assert!(RouteError::BadDatabaseChar('?').to_string().contains('?'));
}

#[test]
fn route_error_is_a_std_error() {
    let err: Box<dyn Error> = Box::new(RouteError::EmptyDatabase);
    assert!(!err.to_string().is_empty());
    assert!(err.source().is_none());
}
