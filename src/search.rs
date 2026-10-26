/*
    Engine: Heron
    Author: ArpaDeveloper

    File: search.rs
    This file handles searching for moves; Negamax with Alpha-Beta Pruning, Iterative Deepening, Quiescense Search, and other optimizations included.
*/

use crate::board::{Board, Color, PieceType};
use crate::evaluation::{evaluate, is_insufficient_material};
use crate::movegen::{in_check, is_attacked, legal_moves, pseudo_legal_captures, pseudo_legal_moves, Move, MoveKind, MoveList, MAX_MOVES};
use crate::transpositiontable::{Bound, TranspositionTable};

use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

//Constants
pub const MATE_SCORE: i32 = 100_000;
const INF: i32 = 1_000_000;
const MAX_PLY: i32 = 128;
const DELTA_MARGIN: i32 = 200;
const PIECE_VALUES: [i32; 6] = [100, 320, 330, 500, 900, 0];
const MATE_BOUND: i32 = MATE_SCORE - 1000;
const KILLER_SLOTS: usize = 128;
const HISTORY_MAX: i32 = 60_000; 
const RFP_MARGIN: i32 = 100;
const FUTILITY_MARGIN: i32 = 120;  
const ASPIRATION_WINDOW: i32 = 40;

//LIMITS
#[derive(Copy, Clone, Debug)]
pub struct Limits{
    pub soft: Option<Duration>,
    pub hard: Option<Duration>,
    pub nodes: Option<u64>,
}

impl Limits{
    //No limits
    pub const fn none() -> Limits{
        Limits{ 
            soft: None, 
            hard: None,
            nodes: None
        } 
    }

    //Subtract time that has already been used
    pub fn shrink(self,used: Duration)->Limits{
        Limits {
            soft: self.soft.map(|d| d.saturating_sub(used)),
            hard: self.hard.map(|d| d.saturating_sub(used)),
            nodes: self.nodes,
        }
    }
}

//SEARCHINFO (TIME LIMITS + TRANSPOSITION TABLES)
pub struct SearchInfo<'a>{
    start: Instant,
    hard_limit: Option<Duration>,
    node_limit: Option<u64>,
    stop: Arc<AtomicBool>,
    nodes: u64,
    stopped: bool,
    tt: &'a mut TranspositionTable,
    killers: [[Option<Move>; 2]; KILLER_SLOTS], 
    history: [[[i32; 64]; 64]; 2],            
}

impl<'a> SearchInfo<'a>{
    //Creates new search state
    pub fn new(limits:Limits,tt:&'a mut TranspositionTable,stop:Arc<AtomicBool>)->SearchInfo<'a>{
        SearchInfo{
            start: Instant::now(),
            hard_limit: limits.hard,
            node_limit: limits.nodes,
            stop,
            nodes: 0,
            stopped: false,
            tt,
            killers: [[None; 2]; KILLER_SLOTS],
            history: [[[0; 64]; 64]; 2],
        }
    }

    //Checks the time limit
    fn check_time(&mut self){
        if self.nodes & 255 == 0{
            if self.stop.load(Ordering::Relaxed){
                self.stopped = true;
            }else if self.node_limit.map_or(false, |n| self.nodes >= n){
                self.stopped = true;
            } else if let Some(limit) = self.hard_limit{
                if self.start.elapsed() >= limit { self.stopped = true; }
            }
        }
    }

    //Converts the search ply into a valid killer-move table index
    fn slot(ply:i32)->usize{
        (ply.max(0) as usize).min(KILLER_SLOTS - 1)
    }

    //Records a quiet move that caused a beta cutoff
    fn record_cutoff(&mut self,side:usize,m:Move,depth:u32,ply:i32){
        let p = Self::slot(ply);
        if self.killers[p][0] != Some(m){
            self.killers[p][1] = self.killers[p][0];
            self.killers[p][0] = Some(m);
        }

        let h = &mut self.history[side][m.from().0 as usize][m.to().0 as usize];
        *h += (depth * depth) as i32;
        if *h > HISTORY_MAX{
            for s in self.history.iter_mut(){
                for f in s.iter_mut() {
                    for t in f.iter_mut() { *t /= 2; }
                }
            }
        }
    }
}

//NEGAMAX + ALPHA-BETA PRUNING
fn alpha_beta(board:&mut Board, depth:u32, mut alpha:i32, beta:i32, ply:i32, info:&mut SearchInfo, allow_null:bool)->i32{
    info.nodes += 1;
    info.check_time();
    if info.stopped { return 0; }

    if ply > 0 && (is_insufficient_material(board) || board.is_repetition(ply)){
        return 0;
    }

    let in_chk = in_check(board);
    if ply > 0 && board.halfmove_clock >= 100 && (!in_chk || !legal_moves(board).is_empty()){
        return 0;
    }

    let mut depth = depth;
    if in_chk && ply < MAX_PLY{
        depth += 1;
    }
    if depth == 0{
        return quiescence(board, alpha, beta, ply, info);
    }

    let is_pv = beta - alpha > 1;

    let hash = board.hash;
    let mut tt_move = None;
    if let Some(e) = info.tt.probe(hash){
        tt_move = e.best;
        if ply > 0 && board.halfmove_clock < 90 && e.depth as u32 >= depth{
            let s = score_from_tt(e.score, ply);
            match e.bound{
                Bound::Exact => return s.max(alpha).min(beta),
                Bound::Lower if s >= beta => return beta,
                Bound::Upper if s <= alpha => return alpha,
                _ => {}
            }
        }
    }

    let can_prune = !in_chk && !is_pv && beta.abs() < MATE_BOUND;
    let static_eval = if can_prune { evaluate(board) } else { 0 };

    if can_prune && depth <= 6 && static_eval - RFP_MARGIN * depth as i32 >= beta{
        return beta;
    }

    if can_prune && allow_null && depth >= 3 && static_eval >= beta && has_non_pawn_material(board, board.side_to_move){
        let r = 2 + depth / 6;
        let undo = board.make_null_move();
        let score = -alpha_beta(board, (depth - 1).saturating_sub(r), -beta, -beta + 1, ply + 1, info, false);
        board.unmake_null_move(undo);
        if info.stopped { return 0; }
        if score >= beta { return beta; }
    }

    let us = board.side_to_move;
    let mut list = pseudo_legal_moves(board);
    let mut scores = score_moves(board, &list, tt_move, ply, info);

    let alpha_orig = alpha;
    let mut best_move: Option<Move> = None;
    let side = us as usize;
    let mut legal = 0usize;

    for i in 0..list.len(){
        let m = pick_next(&mut list, &mut scores, i);
        let quiet = !is_tactical(m);
        let undo = board.make_move(m);
        if king_attacked(board, us){
            board.unmake_move(m, undo);
            continue;
        }
        let idx = legal;
        legal += 1;

        let gives_check = quiet && idx > 0 && in_check(board);

        if can_prune && idx > 0 && quiet && !gives_check && depth <= 3 && alpha.abs() < MATE_BOUND
            && static_eval + FUTILITY_MARGIN * depth as i32 <= alpha{
            board.unmake_move(m, undo);
            continue;
        }

        let score = if idx == 0{
            -alpha_beta(board, depth - 1, -beta, -alpha, ply + 1, info, true)
        }else{
            let mut s;
            if depth >= 3 && idx >= 3 && quiet && !in_chk && !gives_check{
                let r = 1 + (idx >= 8) as u32 + (depth >= 7) as u32;
                let reduced = (depth - 1).saturating_sub(r).max(1);
                s = -alpha_beta(board, reduced, -alpha - 1, -alpha, ply + 1, info, true);
                if s > alpha{
                    s = -alpha_beta(board, depth - 1, -alpha - 1, -alpha, ply + 1, info, true);
                }
            }else{
                s = -alpha_beta(board, depth - 1, -alpha - 1, -alpha, ply + 1, info, true);
            }
            if s > alpha && s < beta{
                s = -alpha_beta(board, depth - 1, -beta, -alpha, ply + 1, info, true);
            }
            s
        };
        board.unmake_move(m, undo);

        if info.stopped { return 0; }

        if score >= beta{
            if quiet { info.record_cutoff(side, m, depth, ply); }
            info.tt.store(hash, depth, score_to_tt(beta, ply), Bound::Lower, Some(m));
            return beta;
        }
        if score > alpha{
            alpha = score;
            best_move = Some(m);
        }
    }

    if legal == 0{
        return if in_chk { -MATE_SCORE + ply } else { 0 };
    }

    let bound = if alpha > alpha_orig { Bound::Exact } else { Bound::Upper };
    info.tt.store(hash, depth, score_to_tt(alpha, ply), bound, best_move);
    alpha
}

//QUIESCENCESEARCH
fn quiescence(board:&mut Board, mut alpha:i32, beta:i32, ply:i32, info:&mut SearchInfo)->i32{
    info.nodes += 1;
    info.check_time();
    if info.stopped { return 0; }

    if ply >= MAX_PLY{
        return evaluate(board);
    }

    let in_chk = in_check(board);
    let mut stand_pat = 0;
    if !in_chk{
        stand_pat = evaluate(board);
        if stand_pat >= beta { return beta; }
        if stand_pat > alpha { alpha = stand_pat; }
    }

    let us = board.side_to_move;
    let mut list = if in_chk { pseudo_legal_moves(board) } else { pseudo_legal_captures(board) };
    let mut scores = [0; MAX_MOVES];
    for (i, m) in list.iter().enumerate(){
        scores[i] = mvv_lva(board, m);
    }

    let mut legal = 0;
    for i in 0..list.len(){
        let m = pick_next(&mut list, &mut scores, i);

        if !in_chk && m.promotion_piece().is_none() && stand_pat + victim_value(board, m) + DELTA_MARGIN < alpha{
            continue;
        }

        let undo = board.make_move(m);
        if king_attacked(board, us){
            board.unmake_move(m, undo);
            continue;
        }
        legal += 1;
        let score = -quiescence(board, -beta, -alpha, ply + 1, info);
        board.unmake_move(m, undo);
        if info.stopped { return 0; }

        if score >= beta { return beta; }
        if score > alpha { alpha = score; }
    }

    if in_chk && legal == 0{
        return -MATE_SCORE + ply;
    }
    alpha
}

//MOVEORDERING
fn move_score(board:&Board,m:Move,tt_move:Option<Move>,ply:i32,info:&SearchInfo)->i32{
    if Some(m) == tt_move{
        return 1_000_000;                         
    }
    if is_tactical(m){
        return 100_000 + mvv_lva(board, m);
    }
    let p = SearchInfo::slot(ply);
    if info.killers[p][0] == Some(m) { return 90_000; }
    if info.killers[p][1] == Some(m) { return 89_000; }
    info.history[board.side_to_move as usize][m.from().0 as usize][m.to().0 as usize]
}


//ROOTSEARCH
fn search_root(board:&mut Board,depth:u32,prev_best:Option<Move>,mut alpha:i32,beta:i32,info:&mut SearchInfo)->Option<(Move,i32)>{
    let mut list = legal_moves(board);
    if list.is_empty() { return None; }
    let tt_move = prev_best.or_else(|| info.tt.probe(board.hash).and_then(|e| e.best));
    let mut scores = score_moves(board, &list, tt_move, 0, info);

    let alpha_orig = alpha;
    let mut best=(list.get(0), -INF);

    for i in 0..list.len(){
        let m = pick_next(&mut list, &mut scores, i);
        if i == 0 { best.0 = m; }
        let undo = board.make_move(m);
        let score = if i == 0 {
            -alpha_beta(board, depth - 1, -beta, -alpha, 1, info, true)
        }else{
            let mut s = -alpha_beta(board, depth - 1, -alpha - 1, -alpha, 1, info, true);
            if s > alpha && s < beta {
                s = -alpha_beta(board, depth - 1, -beta, -alpha, 1, info, true);
            }
            s
        };
        board.unmake_move(m, undo);

        if info.stopped { return None; }
        if score >= beta{ return Some((m, beta)); }
        if score > alpha{
            alpha = score;
            best = (m, score);
        }
    }
    if alpha > alpha_orig{
        info.tt.store(board.hash, depth, best.1, Bound::Exact, Some(best.0));
    }
    Some(best)
}

//Searches with a narrow score window to improve efficiency
fn aspiration_search(board:&mut Board,depth:u32,prev:Option<(Move,i32)>,info:&mut SearchInfo)->Option<(Move,i32)>{
    let prev_move = prev.map(|(m, _)| m);
    let (mut alpha, mut beta, mut window) = match prev{
        Some((_, s)) if depth >= 5 && s.abs() < MATE_BOUND => (s - ASPIRATION_WINDOW, s + ASPIRATION_WINDOW, ASPIRATION_WINDOW),
        _ => (-INF, INF, 0),
    };

    loop {
        let (m, s) = search_root(board, depth, prev_move, alpha, beta, info)?;
        if s <= alpha && alpha > -INF{
            window *= 2;
            alpha = if window >= 640 { -INF } else { alpha - window };
        }else if s >= beta && beta < INF{
            window *= 2;
            beta = if window >= 640 { INF } else { beta + window };
        }else{
            return Some((m, s));
        }
    }
}

//SELECT BEST MOVE
pub fn best_move(board:&mut Board,max_depth:u32,limits:Limits,tt:&mut TranspositionTable,stop:Arc<AtomicBool>)->Option<(Move, i32)>{
    let mut info = SearchInfo::new(limits, tt, stop);
    let mut best: Option<(Move, i32)> = None;
    let mut prev_move: Option<Move> = None;

    //ITERATIVEDEEPENING
    for depth in 1..=max_depth{
        match aspiration_search(board, depth, best, &mut info){
            Some(result) => {
                best = Some(result);
                let score_str = if result.1.abs() > MATE_BOUND{
                    let plies = MATE_SCORE - result.1.abs();
                    let moves = (plies + 1) / 2;
                    format!("mate {}", if result.1 > 0 { moves } else { -moves })
                }else{
                    format!("cp {}", result.1)
                };
                let ms = info.start.elapsed().as_millis().max(1);
                let nps = info.nodes as u128 * 1000 / ms;
                println!("info depth {} score {} nodes {} nps {} time {} pv {}",
                    depth, score_str, info.nodes, nps, ms,
                    extract_pv(board, &*info.tt, 20).iter().map(|m| m.to_uci()).collect::<Vec<_>>().join(" "));

                if result.1.abs() > MATE_BOUND { break; }

                let changed = prev_move.is_some() && prev_move != Some(result.0);
                prev_move = Some(result.0);
                if let Some(soft) = limits.soft{
                    let e = info.start.elapsed();
                    let over = if changed { e > soft } else { e * 2 > soft };
                    if over { break; }
                }
            }
            None => break,
        }
    }
    best.or_else(|| legal_moves(board).iter().next().map(|m| (m, 0)))
}

//HELPERS
//Checks if move is tactical
fn is_tactical(m: Move)->bool{
    matches!(m.kind(),MoveKind::Capture | MoveKind::EnPassant | MoveKind::Promotion(_) | MoveKind::PromotionCapture(_))
}

//Value of the piece this move captures (0 for non-captures)
fn victim_value(board:&Board,m:Move)->i32{
    match m.kind(){
        MoveKind::EnPassant => PIECE_VALUES[PieceType::Pawn as usize],
        _ => board.piece_at(m.to()).map_or(0, |(_, pt)| PIECE_VALUES[pt as usize]),
    }
}

//MVV-LVA: most valuable victim first, least valuable attacker as tie-break
fn mvv_lva(board:&Board,m:Move)->i32{
    let attacker=board.piece_at(m.from()).map_or(0, |(_, pt)| PIECE_VALUES[pt as usize]);
    let promo = m.promotion_piece().map_or(0, |pt| PIECE_VALUES[pt as usize] * 10);
    victim_value(board, m) * 10 - attacker / 10 + promo
}

//Adjusts mate scores before storing them in the transposition table
fn score_to_tt(score:i32,ply:i32)->i32{
    if score > MATE_BOUND{
        score + ply
    }else if score < -MATE_BOUND{
        score - ply
    }else{
        score
    }
}

//Restores the correct mate distance when retrieving a score from the transposition table
fn score_from_tt(score:i32,ply:i32)->i32{
    if score > MATE_BOUND{
        score - ply
    }else if score < -MATE_BOUND{
        score + ply
    }else{
        score
    }
}

//Null move is unsafe in pawn-only endgames (zugzwang)
fn has_non_pawn_material(board:&Board,color:Color)->bool{
    let own = board.all_pieces_by_color[color as usize];
    let pawns_and_king = board.piece_bitboard(color, PieceType::Pawn) | board.piece_bitboard(color, PieceType::King);
    !(own & !pawns_and_king).is_empty()
}

//Principal variation
fn extract_pv(board:&mut Board,tt:&TranspositionTable,max_len:usize)->Vec<Move>{
    let mut pv = Vec::new();
    let mut undos = Vec::new();
    while pv.len() < max_len{
        let Some(m) = tt.probe(board.hash).and_then(|e| e.best) else { break };
        if !legal_moves(board).iter().any(|x| x == m) { break; }
        undos.push((m, board.make_move(m)));
        pv.push(m);
    }
    while let Some((m, u)) = undos.pop(){
        board.unmake_move(m, u);
    }
    pv
}

//Checks if the king is under attack
fn king_attacked(board:&Board,us:Color)->bool{
    board.king_square(us).map_or(false, |k| is_attacked(board, k, us.opposite()))
}

//Assigns scores to moves for move ordering
fn score_moves(board:&Board,list:&MoveList,tt_move:Option<Move>,ply:i32,info:&SearchInfo)->[i32;MAX_MOVES]{
    let mut s = [0; MAX_MOVES];
    for (i, m) in list.iter().enumerate(){
        s[i] = move_score(board, m, tt_move, ply, info);
    }
    s
}

//Selects the highest-scoring move and moves it to the front
fn pick_next(list:&mut MoveList,scores:&mut [i32;MAX_MOVES],start:usize)->Move{
    let mut best = start;
    for j in start + 1..list.len(){
        if scores[j] > scores[best] { best = j; }
    }
    scores.swap(start, best);
    list.swap(start, best);
    list.get(start)
}