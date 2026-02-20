use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "command_executor/command_grammar.pest"]
pub struct CommandParser;
