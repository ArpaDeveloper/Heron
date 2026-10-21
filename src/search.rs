// search.rs — Search algorithms
use crate::board::Board;
use crate::evaluation::evaluate;
use crate::movegen::{in_check, legal_moves, Move};

use std::time::{Duration, Instant};

pub const MATE_SCORE: i32 = 100_000;
const INF: i32 = 1_000_000;

//SEARCHINFO (TIME LIMITS)
pub struct SearchInfo{
    start: Instant,
    time_limit: Option<Duration>, //None = no time limit
    nodes: u64,
    stopped: bool,
}

impl SearchInfo{
    //Creates new time_limit
    pub fn new(time_limit:Option<Duration>)->SearchInfo {
        SearchInfo {start: Instant::now(), time_limit, nodes: 0, stopped: false}
    }

    //Checks the clock
    fn check_time(&mut self){
        if self.nodes & 255 == 0{
            if let Some(limit) = self.time_limit{
                if self.start.elapsed() >= limit{
                    self.stopped = true;
                }
            }
        }
    }
}

//NEGAMAX + ALPHA-BETA PRUNING
fn alpha_beta(board:&mut Board, depth:u32, mut alpha:i32, beta:i32, ply:i32,info: &mut SearchInfo)->i32{
    info.nodes += 1;                          
    info.check_time();                      
    if info.stopped { return 0; } 

    if depth==0{
        return evaluate(board);
    }

    let moves = legal_moves(board);
    if moves.is_empty(){
        return if in_check(board){
            -MATE_SCORE+ply //Checkmate
        }else{
            0 //Stalemate
        };
    }

    for m in moves.iter(){
        let undo = board.make_move(m);
        let score=-alpha_beta(board,depth-1,-beta,-alpha,ply+1,info);
        board.unmake_move(m, undo);

        if info.stopped { return 0; }

        if score >= beta{//Prune (Better option for opponent elsewhere)
            return beta;
        }
        if score > alpha{
            alpha = score;
        }
    }
    alpha
}

//ROOTSEARCH
pub fn search_root(board:&mut Board,depth:u32, prev_best: Option<Move>,info: &mut SearchInfo)->Option<(Move, i32)>{
    let mut moves: Vec<Move> = legal_moves(board).iter().collect();
    if let Some(pb) = prev_best{
        if let Some(i) = moves.iter().position(|&m| m == pb){
            moves.swap(0, i);
        }
    }

    let mut best:Option<(Move,i32)>=None;
    let mut alpha=-INF;
    let beta=INF;

    for m in moves{
        let undo = board.make_move(m);
        let score = -alpha_beta(board, depth - 1, -beta, -alpha, 1, info);
        board.unmake_move(m, undo);

        if info.stopped { return None; }

        if score > alpha{
            alpha = score;
            best = Some((m, score));
        }
    }
    best
}

//SELECT BEST MOVE
pub fn best_move(board:&mut Board,max_depth:u32,time_limit:Option<Duration>)->Option<(Move, i32)>{
    let mut info = SearchInfo::new(time_limit);
    let mut best: Option<(Move, i32)> = None;

    //ITERATIVE DEEPENING
    for depth in 1..=max_depth{
        let prev = best.map(|(m, _)| m);
        match search_root(board, depth, prev, &mut info){
            Some(result) => {
                best = Some(result);
                let score_str = if result.1.abs() > MATE_SCORE - 1000{
                    let plies = MATE_SCORE - result.1.abs();
                    let moves = (plies + 1) / 2;
                    format!("mate {}", if result.1 > 0 { moves } else { -moves })
                }else{
                    format!("cp {}", result.1)
                };

                println!("info depth {} score {} nodes {} time {} pv {}",depth, score_str, info.nodes,info.start.elapsed().as_millis(), result.0.to_uci());
                if result.1.abs() > MATE_SCORE - 1000 { break; }
                if let Some(limit) = time_limit {
                    if info.start.elapsed() * 2 > limit { break; }
                }
            }
            None => break,
        }
    }
    best.or_else(|| legal_moves(board).iter().next().map(|m| (m, 0)))
}







// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------
// - A search entry point that takes: the board, search limits (max depth,
//   time budget in ms, node limit, infinite), and a shared stop flag.
//   It returns the best move (and optionally the score and principal
//   variation).
// - A Limits/SearchInfo struct holding the parameters parsed from the UCI
//   "go" command and the time budget computed by main.rs.
// - Search state kept across a single search: node counter, start time,
//   killer moves, history table, principal variation table.
//
// ---------------------------------------------------------------------------
// Core algorithm: negamax with alpha-beta pruning
// ---------------------------------------------------------------------------
// - Scores are from the side to move's point of view; the child score is
//   negated and the window flipped.
// - Terminal conditions at a node:
//     * no legal moves: checkmate (score = -MATE + ply, so shorter mates
//       are preferred) or stalemate (score = 0)
//     * draw by repetition, 50-move rule, insufficient material: score 0
//     * depth reached: go into quiescence search
// - Define constants for INFINITY, MATE, and a "mate in N" threshold so
//   mate scores can be recognized and printed as "score mate N".
//
// ---------------------------------------------------------------------------
// Iterative deepening
// ---------------------------------------------------------------------------
// - Search depth 1, 2, 3, ... until the depth limit or the time is up.
// - Keep the best move from the last COMPLETED iteration; if the search is
//   interrupted mid-iteration, return that move.
// - After each completed iteration, print a UCI "info" line:
//   depth, score, nodes, nps, time, pv.
// - Always make sure a legal move is returned, even at depth 1 with almost
//   no time left.
//
// ---------------------------------------------------------------------------
// Quiescence search
// ---------------------------------------------------------------------------
// - At depth 0, keep searching captures (and promotions) until the position
//   is quiet, to avoid the horizon effect.
// - Stand-pat: use the static evaluation as a lower bound; if it already
//   beats beta, return it.
// - Optional: delta pruning, skip captures with a bad SEE.
//
// ---------------------------------------------------------------------------
// Move ordering (this matters more than almost anything else)
// ---------------------------------------------------------------------------
// - Order moves before searching them, best first:
//     1. transposition table move / principal variation move
//     2. captures sorted by MVV-LVA (most valuable victim, least valuable
//        attacker); promotions near the front
//     3. killer moves (quiet moves that caused a beta cutoff at the same ply)
//     4. quiet moves sorted by history heuristic score
// - Update killers and history when a quiet move causes a beta cutoff.
//
// ---------------------------------------------------------------------------
// Pruning and reductions (add one at a time, test each for strength)
// ---------------------------------------------------------------------------
// - Check extension: search one ply deeper when in check.
// - Null move pruning: skip a turn and search at reduced depth; if the
//   score still beats beta, prune. Not in check, not in pawn-only endgames.
// - Late move reductions: search moves late in the ordered list at reduced
//   depth, re-search at full depth if they surprise.
// - Principal variation search: full window for the first move, a null
//   window for the rest, re-search on fail-high.
// - Aspiration windows around the previous iteration's score.
// - Optional: futility pruning, reverse futility pruning.
//
// ---------------------------------------------------------------------------
// Transposition table
// ---------------------------------------------------------------------------
// - Fixed-size table indexed by (Zobrist hash % size).
// - Entry: hash key (or part of it), depth, score, bound type
//   (exact / lower / upper), best move.
// - Probe at the start of each node: use the stored score for a cutoff if
//   the depth is sufficient and the bound allows; always use the stored move
//   for ordering.
// - Store at the end of each node. Adjust mate scores by the ply when
//   storing and reading.
// - Cleared on "ucinewgame"; size configurable via the UCI "Hash" option
//   (optional).
//
// ---------------------------------------------------------------------------
// Time management
// ---------------------------------------------------------------------------
// - Check the clock and the stop flag every ~2048 nodes, not at every node.
// - Abort the search cleanly when time is up or "stop" was received; the
//   score of an aborted iteration must be discarded.
// - Hard limit (never exceed) and soft limit (do not start a new iteration
//   if less than roughly half of the time remains).
//
// ---------------------------------------------------------------------------
// Tests to write
// ---------------------------------------------------------------------------
// - Finds mate in 1 and mate in 2 positions.
// - Avoids hanging a queen in a simple tactic.
// - Returns a legal move in positions with a single legal move.
// - Same result with and without the transposition table at a fixed depth
//   (differences hint at hash bugs).