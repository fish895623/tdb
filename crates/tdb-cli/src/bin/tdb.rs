use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, ValueEnum};
use tdb_lexer::{Lexer, Model, TokenType};
use tdb_protocol::{
    DEFAULT_PORT, Endpoint, EndpointError, Request, Response, Route, SCHEME, VERSION,
};

/// One statement per route (`/keyval/..`, `/rdbms/..`, `/vector/..`,
/// `/graph/..`), to show how each model is tokenized.
const STATEMENTS: &[(Model, &str)] = &[
    (Model::Keyval, "SET user:1 \"Ada\"\n"),
    (
        Model::Rdbms,
        "SELECT name FROM users WHERE id = 1 ORDER BY score DESC LIMIT 5;",
    ),
    (
        Model::Vector,
        "SELECT id FROM docs ORDER BY embedding <-> [0.1, 0.2, 0.3] LIMIT 3;",
    ),
    (
        Model::Graph,
        "MATCH (a:Person)-[:KNOWS]->(b) RETURN b.name;",
    ),
];

/// How long to wait for a connection, and then on the server for any one read
/// or write.
const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// The database a statement goes to when the route is not spelled out.
const DEFAULT_DATABASE: &str = "main";

/// The tdb command-line client: sends a statement to a tdbd
#[derive(Debug, Parser)]
#[command(
    name = "tdb",
    version,
    after_help = "The statement goes to a tdbd over the tdb protocol \
                  (docs/dd-005-wire-protocol.md): to 127.0.0.1:7878, or to the --url.\n\
                  \n\
                  A route is the model and database the statement is for. Without one in the \
                  URL, the model is --model, or else the one the statement's first word \
                  belongs to (GET is keyval, MATCH is graph, SELECT is rdbms; rdbms if the \
                  word does not say), and the database is main.\n\
                  \n\
                  For now a statement is only tokenized, by the server or with --local; \
                  running it comes later. With no statement, one built-in statement per \
                  model is tokenized here."
)]
struct Args {
    /// The model to ask for, so the statement is lexed in its dialect
    #[arg(short, long, value_enum)]
    model: Option<ModelArg>,

    /// The tdbd to send to: tdb://HOST[:PORT], optionally followed by the
    /// route /MODEL/DATABASE [default: tdb://127.0.0.1:7878]
    #[arg(short, long, value_name = "URL", requires = "statement")]
    url: Option<Target>,

    /// Lex the statement here instead of sending it to a tdbd
    #[arg(long, conflicts_with = "url")]
    local: bool,

    /// The statement to send
    statement: Option<String>,
}

/// Where a statement is sent: `tdb://HOST[:PORT]`, and optionally the route,
/// `/MODEL/DATABASE`. Without a route the model and database are chosen from
/// the rest of the command line, so the URL of a tdbd is enough to talk to it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    host: String,
    port: u16,
    route: Option<Route>,
}

impl Target {
    /// Where `tdbd` listens unless told otherwise.
    fn default_server() -> Target {
        Target {
            host: "127.0.0.1".to_owned(),
            port: DEFAULT_PORT,
            route: None,
        }
    }

    /// The endpoint for `statement`: the route in the URL if it has one, and
    /// otherwise `model` (or, failing that, the statement's own) in the
    /// default database.
    fn endpoint(self, model: Option<Model>, statement: &str) -> Result<Endpoint> {
        let route = match self.route {
            Some(route) => route,
            None => {
                let model = model.unwrap_or_else(|| detect_model(statement));
                Route::new(model, DEFAULT_DATABASE)?
            }
        };
        Ok(Endpoint::new(self.host, self.port, route))
    }
}

impl FromStr for Target {
    type Err = EndpointError;

    fn from_str(url: &str) -> Result<Target, EndpointError> {
        let has_route = url
            .strip_prefix(SCHEME)
            .and_then(|rest| rest.strip_prefix("://"))
            .is_some_and(|rest| rest.contains('/'));
        // `Endpoint` insists on a route, so a bare address is given a
        // placeholder one for the parse and loses it again.
        let endpoint: Endpoint = if has_route {
            url.parse()?
        } else {
            format!("{url}/keyval/_").parse()?
        };
        Ok(Target {
            host: endpoint.host().to_owned(),
            port: endpoint.port(),
            route: has_route.then(|| endpoint.route().clone()),
        })
    }
}

/// The model a statement is for, going by its first word as DD-003
/// describes: a keyword that belongs to one model says which. A shared word
/// or a name does not, and then it is `rdbms`. (The first word cannot tell
/// `rdbms` from `vector`, which both start with `SELECT`: ask for that with
/// `--model` or a route.)
fn detect_model(statement: &str) -> Model {
    let first = Lexer::new(statement)
        .tokenize()
        .ok()
        .and_then(|tokens| tokens.into_iter().next());
    match first.map(|token| token.kind) {
        Some(TokenType::Keyword(keyword)) => keyword.model().unwrap_or(Model::Rdbms),
        _ => Model::Rdbms,
    }
}

/// The models by their route names. `Model` itself lives in `tdb-lexer`,
/// which does not depend on `clap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ModelArg {
    Keyval,
    Rdbms,
    Vector,
    Graph,
}

impl From<ModelArg> for Model {
    fn from(arg: ModelArg) -> Model {
        match arg {
            ModelArg::Keyval => Model::Keyval,
            ModelArg::Rdbms => Model::Rdbms,
            ModelArg::Vector => Model::Vector,
            ModelArg::Graph => Model::Graph,
        }
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let model = args.model.map(Model::from);

    let Some(statement) = args.statement else {
        // (clap refuses --url without a statement.)
        return demo(model);
    };
    if args.local {
        let lexer = match model {
            Some(model) => Lexer::for_model(&statement, model),
            None => Lexer::new(&statement),
        };
        return print_tokens(lexer);
    }

    if model.is_some() && args.url.as_ref().is_some_and(|t| t.route.is_some()) {
        Args::command()
            .error(
                ErrorKind::ArgumentConflict,
                "--model cannot be combined with a --url that has a route: \
                 put the model in the route, or leave the route out",
            )
            .exit();
    }
    let target = args.url.unwrap_or_else(Target::default_server);
    send(&target.endpoint(model, &statement)?, &statement)
}

/// Tokenize one built-in statement per model, or just `model`'s.
fn demo(model: Option<Model>) -> Result<()> {
    for (demo_model, statement) in STATEMENTS {
        if model.is_some_and(|m| m != *demo_model) {
            continue;
        }
        println!("[{demo_model}] {}", statement.trim_end());
        print_tokens(Lexer::for_model(statement, *demo_model))?;
    }
    Ok(())
}

fn print_tokens(lexer: Lexer) -> Result<()> {
    for token in lexer.tokenize()? {
        println!("  {}: {:?}", token.kind, token.value);
    }
    Ok(())
}

/// Connect to `endpoint`, say hello with its route, send `statement` and
/// print the tokens the server answers with.
fn send(endpoint: &Endpoint, statement: &str) -> Result<()> {
    let mut stream = connect(endpoint)?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;

    Request::Hello {
        version: VERSION,
        route: endpoint.route().clone(),
    }
    .write_to(&mut stream)
    .context("sending the hello")?;
    match read_response(&mut stream)? {
        Response::HelloOk { .. } => {}
        Response::Error { message, .. } => bail!("the server refused {endpoint}: {message}"),
        other => bail!("unexpected reply to the hello: {other:?}"),
    }

    Request::Query {
        statement: statement.to_owned(),
    }
    .write_to(&mut stream)
    .context("sending the statement")?;
    match read_response(&mut stream)? {
        Response::Tokens(tokens) => {
            for token in tokens {
                println!("  {}: {:?}", token.kind, token.value);
            }
            Ok(())
        }
        Response::Error { message, .. } => bail!("{message}"),
        other => bail!("unexpected reply to the statement: {other:?}"),
    }
}

/// Connect to the endpoint's host, trying each address it resolves to, each
/// for at most [`IO_TIMEOUT`] so a host that drops packets cannot hang the
/// command.
fn connect(endpoint: &Endpoint) -> Result<TcpStream> {
    let (host, port) = (endpoint.host(), endpoint.port());
    let addresses = (host, port)
        .to_socket_addrs()
        .with_context(|| format!("resolving {host}"))?;
    let mut last_error: Option<io::Error> = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, IO_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(e) => last_error = Some(e),
        }
    }
    Err(match last_error {
        Some(e) => anyhow::Error::new(e).context(format!(
            "connecting to {host}:{port} (is tdbd running there?)"
        )),
        None => anyhow!("{host} did not resolve to any address"),
    })
}

fn read_response(stream: &mut TcpStream) -> Result<Response> {
    Response::read_from(stream)
        .context("reading the reply")?
        .context("the server closed the connection without replying")
}
