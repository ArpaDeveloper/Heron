/*
    Engine: Heron
    Author: ArpaDeveloper

    File: main.rs
    This file is entrypoint to the program.
*/

mod board;
mod evaluation;
mod fen;
mod movegen;
mod search;
mod uci;
mod zobrist;
mod transpositiontable;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    //Usage: main perft <depth> [fen]
    if args.get(1).map(String::as_str) == Some("perft"){
        let depth: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
        let fen = if args.len() > 3 { args[3..].join(" ") } else { fen::START_FEN.to_string() };
        match board::Board::from_fen(&fen) {
            Ok(mut b) => { movegen::perft_divide(&mut b, depth); }
            Err(e) => eprintln!("{}", e),
        }
        return;
    }
    uci::run();
}
