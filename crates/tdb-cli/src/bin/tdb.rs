use std::io::{self, BufRead, IsTerminal, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use clap::{CommandFactory, Parser, ValueEnum};
use tdb_lexer::{Lexer, Model, TokenType};
use tdb_protocol::{
    DEFAULT_PORT, Endpoint, EndpointError, ErrorKind, Request, Response, Route, SCHEME, VERSION,
    WireToken,
};

/// How long to wait for a connection, and then on the server for any one read
/// or write.
const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// The database a statement goes to when the route is not spelled out.
const DEFAULT_DATABASE: &str = "main";

/// The tdb command-line client: sends statements to a tdbd, the server
#[derive(Debug, Parser)]
#[command(
    name = "tdb",
    version,
    after_help = "A statement goes to a tdbd over the tdb protocol \
                  (docs/dd-005-wire-protocol.md): to 127.0.0.1:7878, or to the --url.\n\
                  \n\
                  A route is the model and database the statement is for. Without one in the \
                  URL, the model is --model, or else the one the statement's first word \
                  belongs to (GET is keyval, MATCH is graph, SELECT is rdbms; rdbms if the \
                  word does not say), and the database is main.\n\
                  \n\
                  With no statement, tdb is a shell: it reads statements from standard input, \
                  many to one connection, until `quit` or the end of the input. A statement \
                  ends with `;` at the end of a line (a keyval statement ends at the end of \
                  its line). `help` lists the shell's commands.\n\
                  \n\
                  For now a statement is only tokenized, by the server or with --local; \
                  running it comes later."
)]
struct Args {
    /// The model to ask for, so the statement is lexed in its dialect
    #[arg(short, long, value_enum)]
    model: Option<ModelArg>,

    /// The tdbd to send to: tdb://HOST[:PORT], optionally followed by the
    /// route /MODEL/DATABASE [default: tdb://127.0.0.1:7878]
    #[arg(short, long, value_name = "URL")]
    url: Option<Target>,

    /// Lex the statement here instead of sending it to a tdbd
    #[arg(long, requires = "statement", conflicts_with = "url")]
    local: bool,

    /// The statement to send, and exit [default: read statements from standard
    /// input]
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
        // (clap refuses --local without a statement.)
        let target = target_of(args.url, model);
        let stdin = io::stdin();
        return Shell::new(target, model, stdin.is_terminal())?.run(stdin.lock());
    };
    if args.local {
        let lexer = match model {
            Some(model) => Lexer::for_model(&statement, model),
            None => Lexer::new(&statement),
        };
        return print_tokens(lexer);
    }

    let target = target_of(args.url, model);
    send(&target.endpoint(model, &statement)?, &statement)
}

/// The tdbd to talk to: the `--url`, or the default address. A usage error
/// if `--model` and the URL's route would both say the model.
fn target_of(url: Option<Target>, model: Option<Model>) -> Target {
    if model.is_some() && url.as_ref().is_some_and(|t| t.route.is_some()) {
        Args::command()
            .error(
                clap::error::ErrorKind::ArgumentConflict,
                "--model cannot be combined with a --url that has a route: \
                 put the model in the route, or leave the route out",
            )
            .exit();
    }
    url.unwrap_or_else(Target::default_server)
}

fn print_tokens(lexer: Lexer) -> Result<()> {
    for token in lexer.tokenize()? {
        println!("  {}: {:?}", token.kind, token.value);
    }
    Ok(())
}

fn print_wire_tokens(tokens: &[WireToken]) {
    for token in tokens {
        println!("  {}: {:?}", token.kind, token.value);
    }
}

/// Connect to `endpoint`, say hello with its route, send `statement` and
/// print the tokens the server answers with.
fn send(endpoint: &Endpoint, statement: &str) -> Result<()> {
    match Session::open(endpoint)?.query(statement)? {
        Response::Tokens(tokens) => {
            print_wire_tokens(&tokens);
            Ok(())
        }
        Response::Error { message, .. } => bail!("{message}"),
        other => bail!("unexpected reply to the statement: {other:?}"),
    }
}

/// A connection to a tdbd that has said hello. Statements go one at a time,
/// each read back before the next is sent.
struct Session {
    stream: TcpStream,
}

impl Session {
    /// Connect to `endpoint` and say hello with its route.
    fn open(endpoint: &Endpoint) -> Result<Session> {
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
            Response::HelloOk { .. } => Ok(Session { stream }),
            Response::Error { message, .. } => bail!("the server refused {endpoint}: {message}"),
            other => bail!("unexpected reply to the hello: {other:?}"),
        }
    }

    /// Send one statement and read its reply. `Err` is a failed connection;
    /// the server's own refusals come back as [`Response::Error`].
    fn query(&mut self, statement: &str) -> Result<Response> {
        Request::Query {
            statement: statement.to_owned(),
        }
        .write_to(&mut self.stream)
        .context("sending the statement")?;
        read_response(&mut self.stream)
    }
}

/// What `help` prints.
const SHELL_HELP: &str = "\
Statements end with `;` at the end of a line. A keyval statement ends at the
end of its line.

  \\use /MODEL/DATABASE   talk to another model and database
  \\c                     drop the statement being typed
  help, \\h               this text
  quit, exit, \\q         leave";

/// `tdb` with no statement: statements read from `input`, all over one connection
/// to a tdbd, which is opened by the first statement and reopened if it is
/// lost.
///
/// The route is the URL's, or else `--model`'s, or else (as for a single
/// statement) the model the first statement's first word belongs to; it stays
/// that for the session unless `\use` changes it. On a terminal a failed
/// statement is reported and the session goes on. On a pipe or a file the
/// first failure ends it with a failing status, so a script does not carry on
/// past a statement that did not work.
struct Shell {
    target: Target,
    model: Option<Model>,
    /// Where the session connects, once that is known.
    endpoint: Option<Endpoint>,
    session: Option<Session>,
    terminal: bool,
}

impl Shell {
    fn new(target: Target, model: Option<Model>, terminal: bool) -> Result<Shell> {
        // A route in the URL, or --model, settles it before any statement.
        let endpoint = (model.is_some() || target.route.is_some())
            .then(|| target.clone().endpoint(model, ""))
            .transpose()?;
        Ok(Shell {
            target,
            model,
            endpoint,
            session: None,
            terminal,
        })
    }

    fn run(mut self, mut input: impl BufRead) -> Result<()> {
        if self.terminal {
            eprintln!(
                "tdb {}: `help` lists the commands, `quit` leaves.",
                env!("CARGO_PKG_VERSION")
            );
        }
        let mut statement = String::new();
        loop {
            self.prompt(statement.is_empty());
            let mut line = String::new();
            if input.read_line(&mut line).context("reading input")? == 0 {
                if self.terminal {
                    println!();
                }
                // A last statement without its `;` is still a statement.
                if !statement.trim().is_empty() {
                    let result = self.execute(&statement);
                    self.report(result)?;
                }
                return Ok(());
            }

            if statement.is_empty() {
                if line.trim().is_empty() {
                    continue;
                }
                match Command::parse(&line) {
                    Some(Command::Quit) => return Ok(()),
                    Some(command) => {
                        let result = self.command(command);
                        self.report(result)?;
                        continue;
                    }
                    None => {}
                }
            }

            if !statement.is_empty() && line.trim() == "\\c" {
                statement.clear();
                continue;
            }

            statement.push_str(&line);
            if is_complete(&statement, self.model_of(&statement)) {
                let result = self.execute(&statement);
                self.report(result)?;
                statement.clear();
            }
        }
    }

    /// Show the prompt, if there is someone to read it.
    fn prompt(&self, first_line: bool) {
        if !self.terminal {
            return;
        }
        match (&self.endpoint, first_line) {
            (Some(endpoint), true) => print!("tdb {}> ", endpoint.route()),
            (None, true) => print!("tdb> "),
            (_, false) => print!("   -> "),
        }
        // A prompt has no newline, so it needs pushing out. If the terminal is
        // gone the next read says so.
        let _ = io::stdout().flush();
    }

    /// The model `text` is for: the session's, or else the one its first word
    /// belongs to.
    fn model_of(&self, text: &str) -> Model {
        match &self.endpoint {
            Some(endpoint) => endpoint.route().model(),
            None => detect_model(text),
        }
    }

    /// A failure is shown and forgotten on a terminal, and ends the shell
    /// anywhere else.
    fn report(&self, result: Result<()>) -> Result<()> {
        match result {
            Err(error) if self.terminal => {
                eprintln!("ERROR: {error:#}");
                Ok(())
            }
            other => other,
        }
    }

    /// Send `statement` and print the reply.
    fn execute(&mut self, statement: &str) -> Result<()> {
        let statement = statement.trim_end();
        let reply = self.session(statement)?.query(statement);
        let response = match reply {
            Ok(response) => response,
            Err(error) => {
                // Not knowing what the server got, start over next time.
                self.session = None;
                return Err(error);
            }
        };
        match response {
            Response::Tokens(tokens) => {
                print_wire_tokens(&tokens);
                Ok(())
            }
            Response::Error { kind, message } => {
                // The server hangs up on every error but these two.
                if !matches!(kind, ErrorKind::Lex | ErrorKind::TooLarge) {
                    self.session = None;
                }
                bail!("{message}")
            }
            other => {
                self.session = None;
                bail!("unexpected reply to the statement: {other:?}")
            }
        }
    }

    /// The session, connecting first if there is none. `statement` chooses
    /// the route if nothing has yet.
    fn session(&mut self, statement: &str) -> Result<&mut Session> {
        let session = match self.session.take() {
            Some(session) => session,
            None => {
                let settled = self.endpoint.is_some();
                let endpoint = match self.endpoint.take() {
                    Some(endpoint) => endpoint,
                    None => self.target.clone().endpoint(self.model, statement)?,
                };
                let session = match Session::open(&endpoint) {
                    Ok(session) => session,
                    Err(error) => {
                        // A route the statement chose counts once it has
                        // connected: a server that was not up yet does not
                        // fix the model for the session.
                        self.endpoint = settled.then_some(endpoint);
                        return Err(error);
                    }
                };
                if self.terminal {
                    eprintln!("Connected to {endpoint}");
                }
                self.endpoint = Some(endpoint);
                session
            }
        };
        Ok(self.session.insert(session))
    }

    fn command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Quit => {}
            Command::Help => println!("{SHELL_HELP}"),
            Command::Use(route) => {
                let route: Route = route
                    .parse()
                    .with_context(|| format!("not a route: {route:?} (use /MODEL/DATABASE)"))?;
                self.endpoint = Some(Endpoint::new(
                    self.target.host.clone(),
                    self.target.port,
                    route,
                ));
                self.session = None;
                // Connect now, so a route the server refuses is known now.
                self.session("")?;
            }
            Command::Unknown(word) => bail!("unknown command {word}; `help` lists them"),
        }
        Ok(())
    }
}

/// A line the shell answers itself, instead of sending it. Only the first
/// line of a statement can be one.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Quit,
    Help,
    Use(String),
    /// A backslash command the shell does not have.
    Unknown(String),
}

impl Command {
    fn parse(line: &str) -> Option<Command> {
        let line = line.trim().trim_end_matches(';').trim_end();
        let (word, rest) = match line.split_once(char::is_whitespace) {
            Some((word, rest)) => (word, rest.trim()),
            None => (line, ""),
        };
        match (word.to_ascii_lowercase().as_str(), rest) {
            ("quit" | "exit" | "\\q", "") => Some(Command::Quit),
            ("help" | "\\h" | "\\?", "") => Some(Command::Help),
            ("\\use", route) => Some(Command::Use(route.to_owned())),
            (other, _) if other.starts_with('\\') => Some(Command::Unknown(word.to_owned())),
            _ => None,
        }
    }
}

/// Whether `text` is all of a statement: it ends with `;`, or it is keyval,
/// where a command is a line, as in Redis.
fn is_complete(text: &str, model: Model) -> bool {
    model == Model::Keyval || text.trim_end().ends_with(';')
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
