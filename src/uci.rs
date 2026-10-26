/*
    Engine: Heron
    Author: ArpaDeveloper

    File: uci.rs
    This file handles UCI protocol, commands, and time management
*/

use crate::board::{Board, Color};
use crate::movegen::{legal_moves, perft_divide};
use crate::search::{best_move, Limits};
use crate::transpositiontable::TranspositionTable;

use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

//Constants
const DEFAULT_DEPTH: u32 = 4;
const MAX_DEPTH: u32 = 64;               
const MOVE_OVERHEAD_MS: u64 = 100;        
const DEFAULT_MOVES_TO_GO: u64 = 30;   
const DEFAULT_HASH_MB: usize = 16;

//UCI & THREADS
//Stops a running search
fn halt(stop:&AtomicBool,worker:&mut Option<JoinHandle<()>>){
    stop.store(true, Ordering::Relaxed);
    if let Some(h) = worker.take() { let _ = h.join(); }
}

//Handles UCI commands and manages the engine's search thread
pub fn run(){
    let stdin = io::stdin();
    let mut board = Board::startpos();
    let tt = Arc::new(Mutex::new(TranspositionTable::new(DEFAULT_HASH_MB)));
    let stop = Arc::new(AtomicBool::new(false));
    let mut worker: Option<JoinHandle<()>> = None;
    let mut infinite = false;

    for line in stdin.lock().lines(){
        let Ok(line) = line else { break };
        let mut parts = line.split_whitespace();
        let Some(command) = parts.next() else { continue };

        match command{
            "uci" => {
                println!("id name Heron");
                println!("id author ArpaDeveloper");
                println!("option name Hash type spin default {} min 1 max 1024", DEFAULT_HASH_MB);
                println!("uciok");
            }
            "isready" => println!("readyok"), 
            "ucinewgame" => {
                halt(&stop, &mut worker);
                board = Board::startpos();
                tt.lock().unwrap().clear();
            }
            "setoption" => {
                halt(&stop, &mut worker);
                let args: Vec<&str> = parts.collect();
                if args.len() >= 4 && args[0] == "name" && args[1].eq_ignore_ascii_case("hash") && args[2] == "value"{
                    if let Ok(mb) = args[3].parse::<usize>(){
                        *tt.lock().unwrap() = TranspositionTable::new(mb.clamp(1, 1024));
                    }
                }
            }
            "position" => {
                halt(&stop, &mut worker);
                let args: Vec<&str> = parts.collect();
                if let Some(b) = parse_position(&args){
                    board = b;
                }else{
                    eprintln!("Invalid position command");
                }
            }
            "go" => {
                let args: Vec<&str> = parts.collect();
                if args.first() == Some(&"perft"){
                    halt(&stop, &mut worker);
                    if let Some(d) = args.get(1).and_then(|s| s.parse::<u32>().ok()){
                        let mut b = board.clone();
                        perft_divide(&mut b, d);
                    }
                    continue;
                }
                halt(&stop, &mut worker); 
                let go_start = Instant::now();
                let params = parse_go(&args);

                let mut limits = if params.infinite{
                    Limits::none()
                }else{
                    time_budget(&params, board.side_to_move).shrink(go_start.elapsed())
                };
                limits.nodes = params.nodes;

                let depth = match (params.depth, limits.hard){
                    (Some(d), _) => d,
                    _ if params.infinite || params.nodes.is_some() => MAX_DEPTH,
                    (None, Some(_)) => MAX_DEPTH,
                    (None, None) => DEFAULT_DEPTH,
                };

                infinite = params.infinite;
                let wait_for_stop = params.infinite;
                stop.store(false, Ordering::Relaxed);

                let mut search_board = board.clone();
                let tt = Arc::clone(&tt);
                let stop = Arc::clone(&stop);
                worker = Some(thread::spawn(move || {
                    let result = {
                        let mut table = tt.lock().unwrap();
                        best_move(&mut search_board, depth, limits, &mut *table, Arc::clone(&stop))
                    };
                    if wait_for_stop{
                        while !stop.load(Ordering::Relaxed){
                            thread::sleep(Duration::from_millis(5));
                        }
                    }
                    match result{
                        Some((m, _)) => println!("bestmove {}", m.to_uci()),
                        None => println!("bestmove 0000"),
                    }
                    let _ = io::stdout().flush();
                }));
            }
            "stop" => stop.store(true, Ordering::Relaxed),
            "quit" => {
                halt(&stop, &mut worker);
                break;
            }
            _ => {}
        }
        let _ = io::stdout().flush();
    }
    if infinite { stop.store(true, Ordering::Relaxed); }
    if let Some(h) = worker.take() { let _ = h.join(); }
}

//Parses a position and applies any given moves
pub(crate) fn parse_position(args:&[&str])->Option<Board>{
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
pub(crate) struct GoParams{
    pub(crate) depth: Option<u32>,
    pub(crate) movetime: Option<u64>,  
    pub(crate) wtime: Option<u64>,     
    pub(crate) btime: Option<u64>,
    pub(crate) winc: Option<u64>,     
    pub(crate) binc: Option<u64>,
    pub(crate) movestogo: Option<u64>,
    pub(crate) nodes: Option<u64>,
    pub(crate) infinite: bool,
}

//Parses UCI search limits and time controls
pub(crate) fn parse_go(args: &[&str])->GoParams{
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
            "nodes" => p.nodes = num(next),
            "infinite" => p.infinite = true,
            _ => {}
        }
    }
    p
}

//Turns u64 to milliseconds
fn ms(v:u64)->Duration{
    Duration::from_millis(v)
}

//Handles time budget
pub(crate) fn time_budget(p:&GoParams,side:Color)->Limits{
    if let Some(mt) = p.movetime{
        let t = ms(mt.saturating_sub(MOVE_OVERHEAD_MS).max(1));
        return Limits { soft: Some(t), hard: Some(t),nodes: None };
    }

    let (time_left, inc) = match side{
        Color::White => (p.wtime, p.winc.unwrap_or(0)),
        Color::Black => (p.btime, p.binc.unwrap_or(0)),
    };
    let Some(time) = time_left else { return Limits::none() };

    let moves_left = p.movestogo.unwrap_or(DEFAULT_MOVES_TO_GO).max(1);
    let base = time / moves_left + inc * 3 / 4;

    let cap = time.saturating_sub(MOVE_OVERHEAD_MS) * 8 / 10;
    let hard = (base * 3).min(cap).max(1);
    let soft = base.min(hard).max(1);
    Limits { soft: Some(ms(soft)), hard: Some(ms(hard)),nodes: None }
}