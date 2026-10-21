use crate::board::{Board, Color};
use crate::movegen::legal_moves;
use crate::search::best_move;

use std::io::{self, BufRead, Write};
use std::time::Duration;

const DEFAULT_DEPTH: u32 = 4;
const MAX_DEPTH: u32 = 64;               
const MOVE_OVERHEAD_MS: u64 = 100;        
const DEFAULT_MOVES_TO_GO: u64 = 30;   

pub fn run() {
    let stdin = io::stdin();
    let mut board = Board::startpos();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let mut parts = line.split_whitespace();
        let Some(command) = parts.next() else { continue };

        match command {
            "uci" => {
                println!("id name RustChess");
                println!("id author ArpaDeveloper");
                println!("uciok");
            }
            "isready" => println!("readyok"),
            "ucinewgame" => board = Board::startpos(),
            "position" => {
                let args: Vec<&str> = parts.collect();
                if let Some(b) = parse_position(&args) {
                    board = b;
                } else {
                    eprintln!("Invalid position command");
                }
            }
            "go" => {
                let go_start = std::time::Instant::now();
                let args: Vec<&str> = parts.collect();
                let params = parse_go(&args);
                let limit = time_budget(&params, board.side_to_move);

                let depth = match (params.depth, limit){
                    (Some(d), _) => d,
                    (None, Some(_)) => MAX_DEPTH,
                    (None, None) => DEFAULT_DEPTH,
                };

                //Subtract the time already spent parsing
                let adjusted = limit.map(|l| l.saturating_sub(go_start.elapsed()));

                let result = best_move(&mut board, depth, adjusted);
                eprintln!("budget {:?} used {:?}", limit, go_start.elapsed());

                match result{
                    Some((m, _)) => println!("bestmove {}", m.to_uci()),
                    None => println!("bestmove 0000"),
                }
            }
            "stop" => {}
            "quit" => break,
            _ => {}
        }
        let _ = io::stdout().flush();
    }
}

// Builds a board from "position startpos [moves ...]" or "position fen <6 fields> [moves ...]".
// Returns None if the FEN or any move is invalid, so the caller keeps its old board.
fn parse_position(args:&[&str])->Option<Board>{
    let moves_at = args.iter().position(|&a| a == "moves");
    let setup_end = moves_at.unwrap_or(args.len());

    let mut board = match *args.first()?{
        "startpos" => Board::startpos(),
        "fen" => Board::from_fen(&args[1..setup_end].join(" ")).ok()?,
        _ => return None,
    };

    if let Some(i) = moves_at{
        for text in &args[i + 1..]{
            let m = legal_moves(&mut board)
                .iter()
                .find(|m| m.to_uci() == *text)?;
            board.make_move(m);
        }
    }
    Some(board)
}

//TIMEMANAGEMENT
#[derive(Default)]
struct GoParams{
    depth: Option<u32>,
    movetime: Option<u64>,  
    wtime: Option<u64>,     
    btime: Option<u64>,
    winc: Option<u64>,     
    binc: Option<u64>,
    movestogo: Option<u64>,
}

//Reads "go wtime 60000 btime 60000 winc 1000 binc 1000 ..." style arguments
fn parse_go(args: &[&str])->GoParams{
    let num = |s: Option<&&str>| -> Option<u64>{
        s?.parse::<i64>().ok().map(|v| v.max(0) as u64)
    };

    let mut p = GoParams::default();
    for i in 0..args.len() {
        let next = args.get(i + 1);
        match args[i]{
            "depth" => p.depth = num(next).map(|v| v as u32),
            "movetime" => p.movetime = num(next),
            "wtime" => p.wtime = num(next),
            "btime" => p.btime = num(next),
            "winc" => p.winc = num(next),
            "binc" => p.binc = num(next),
            "movestogo" => p.movestogo = num(next),
            _ => {}
        }
    }
    p
}

//Time Manager
fn time_budget(p: &GoParams, side: Color) -> Option<Duration> {
    if let Some(mt) = p.movetime {
        return Some(Duration::from_millis(mt.saturating_sub(MOVE_OVERHEAD_MS).max(1)));
    }

    let (time, inc) = match side {
        Color::White => (p.wtime?, p.winc.unwrap_or(0)),
        Color::Black => (p.btime?, p.binc.unwrap_or(0)),
    };

    let moves_left = p.movestogo.unwrap_or(DEFAULT_MOVES_TO_GO).max(1);
    let budget = time / moves_left + inc * 3 / 4;

    let cap = time.saturating_sub(MOVE_OVERHEAD_MS) * 8 / 10;
    //Under 2 seconds left: spend very little, just don't flag
    if time < 2000 {
        return Some(Duration::from_millis((time / 40).max(1)));
    }
    Some(Duration::from_millis(budget.min(cap).max(1)))
}