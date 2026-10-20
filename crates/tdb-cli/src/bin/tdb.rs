use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, ValueEnum};
use tdb_lexer::{Lexer, Model};
use tdb_protocol::{Endpoint, Request, Response, VERSION};

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

/// The tdb command-line client
#[derive(Debug, Parser)]
#[command(
    name = "tdb",
    version,
    after_help = "With --url, the statement is sent to a tdbd over the tdb protocol \
                  (docs/dd-005-wire-protocol.md). Without it, the statement is lexed here.\n\
                  For now a statement is only tokenized, either way; running it comes later.\n\
                  With no arguments, one built-in statement per model is tokenized here."
)]
struct Args {
    /// Lex as this model's dialect; by default it is detected per statement
    #[arg(short, long, value_enum, conflicts_with = "url")]
    model: Option<ModelArg>,

    /// Send the statement to this tdbd, as tdb://HOST[:PORT]/MODEL/DATABASE;
    /// the model is the one in the path
    #[arg(short, long, value_name = "URL", requires = "statement")]
    url: Option<Endpoint>,

    /// The statement to tokenize
    statement: Option<String>,
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

    match (args.statement, args.url) {
        (Some(statement), Some(endpoint)) => send(&endpoint, &statement),
        (Some(statement), None) => {
            let lexer = match model {
                Some(model) => Lexer::for_model(&statement, model),
                None => Lexer::new(&statement),
            };
            print_tokens(lexer)
        }
        // (clap refuses --url without a statement.)
        (None, _) => {
            for (demo_model, statement) in STATEMENTS {
                if model.is_some_and(|m| m != *demo_model) {
                    continue;
                }
                println!("[{demo_model}] {}", statement.trim_end());
                print_tokens(Lexer::for_model(statement, *demo_model))?;
            }
            Ok(())
        }
    }
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
        Some(e) => anyhow::Error::new(e).context(format!("connecting to {host}:{port}")),
        None => anyhow!("{host} did not resolve to any address"),
    })
}

fn read_response(stream: &mut TcpStream) -> Result<Response> {
    Response::read_from(stream)
        .context("reading the reply")?
        .context("the server closed the connection without replying")
}
