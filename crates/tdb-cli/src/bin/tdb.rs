use tdb_lexer::{LexError, Lexer};

fn main() -> Result<(), LexError> {
    let commands = "set name = hello\n";

    for token in Lexer::new(commands).tokenize()? {
        println!("{}: {}", token.kind, token.value);
    }
    Ok(())
}
