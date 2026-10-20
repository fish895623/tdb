use tdb_lexer::{LexError, Lexer, Model};

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

fn main() -> Result<(), LexError> {
    for (model, statement) in STATEMENTS {
        println!("[{model}] {}", statement.trim_end());
        for token in Lexer::for_model(statement, *model).tokenize()? {
            println!("  {}: {:?}", token.kind, token.value);
        }
    }
    Ok(())
}
