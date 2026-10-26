/*
    Engine: Heron
    Author: ArpaDeveloper

    File: evaluation.rs
    This file handles position evaluation
*/

use crate::board::{Bitboard, Board, Color, PieceType, Square};
use crate::movegen::{bishop_attacks, king_attacks, knight_attacks, pawn_attacks, queen_attacks, rook_attacks};

use std::ops::{Add, AddAssign, Sub};

//PAWN STRUCTURE WEIGHTS 
const DOUBLED: Score = Score::new(10, 20);       
const ISOLATED: Score = Score::new(10, 15);      
const PROTECTED: Score = Score::new(8, 5);      
const PASSED_MG: [i32; 8] = [0, 5, 10, 20, 35, 60, 100, 0];
const PASSED_EG: [i32; 8] = [0, 10, 20, 40, 70, 120, 180, 0];

//PIECE ACTIVITY WEIGHTS 
const ROOK_SEMI_OPEN: Score = Score::new(10, 10);
const ROOK_OPEN: Score = Score::new(20, 15);
const ROOK_ON_SEVENTH: Score = Score::new(20, 30);
const KNIGHT_OUTPOST: Score = Score::new(20, 10);

//TEMPO AND DRAWS
const TEMPO: i32 = 10;
const DARK_SQUARES: u64 = 0xAA55_AA55_AA55_AA55;

//KING SAFETY WEIGHTS 
const SHIELD_BONUS: i32 = 10;      
const SEMI_OPEN_PENALTY: i32 = 15;  
const OPEN_EXTRA_PENALTY: i32 = 10; 
const MAX_PHASE: i32 = 24;

//SCORE
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Score{
    mg: i32,
    eg: i32
}

impl Score{
    const ZERO: Score = Score { mg: 0, eg: 0 };
    //Create a new score
    const fn new(mg: i32, eg: i32)->Score{
        Score{mg,eg}
    }
}

//Component-wise operations for score
impl Add for Score{
    type Output = Score;
    fn add(self,o:Score)->Score{
        Score::new(self.mg + o.mg, self.eg + o.eg)
    }
}

impl Sub for Score{
    type Output = Score;
    fn sub(self,o:Score)->Score{
        Score::new(self.mg - o.mg, self.eg - o.eg)
    }
}

impl AddAssign for Score{
    fn add_assign(&mut self, o: Score){
        self.mg += o.mg; self.eg += o.eg;
    }
}

//PIECEVALUE (centipawn)
pub(crate) fn piece_value(pt: PieceType)->i32{
    match pt {
        PieceType::Pawn => 100,
        PieceType::Knight => 320,
        PieceType::Bishop => 330,
        PieceType::Rook => 500,
        PieceType::Queen => 900,
        PieceType::King => 0,
    }
}

//PIECE-SQUARE TABLES
#[rustfmt::skip]
const PAWN: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
    50, 50, 50, 50, 50, 50, 50, 50,
    10, 10, 20, 30, 30, 20, 10, 10,
     5,  5, 10, 25, 25, 10,  5,  5,
     0,  0,  0, 20, 20,  0,  0,  0,
     5, -5,-10,  0,  0,-10, -5,  5,
     5, 10, 10,-20,-20, 10, 10,  5,
     0,  0,  0,  0,  0,  0,  0,  0,
];

#[rustfmt::skip]
const KNIGHT: [i32; 64] = [
   -50,-40,-30,-30,-30,-30,-40,-50,
   -40,-20,  0,  0,  0,  0,-20,-40,
   -30,  0, 10, 15, 15, 10,  0,-30,
   -30,  5, 15, 20, 20, 15,  5,-30,
   -30,  0, 15, 20, 20, 15,  0,-30,
   -30,  5, 10, 15, 15, 10,  5,-30,
   -40,-20,  0,  5,  5,  0,-20,-40,
   -50,-40,-30,-30,-30,-30,-40,-50,
];

#[rustfmt::skip]
const BISHOP: [i32; 64] = [
   -20,-10,-10,-10,-10,-10,-10,-20,
   -10,  0,  0,  0,  0,  0,  0,-10,
   -10,  0,  5, 10, 10,  5,  0,-10,
   -10,  5,  5, 10, 10,  5,  5,-10,
   -10,  0, 10, 10, 10, 10,  0,-10,
   -10, 10, 10, 10, 10, 10, 10,-10,
   -10,  5,  0,  0,  0,  0,  5,-10,
   -20,-10,-10,-10,-10,-10,-10,-20,
];

#[rustfmt::skip]
const ROOK: [i32; 64] = [
     0,  0,  0,  0,  0,  0,  0,  0,
     5, 10, 10, 10, 10, 10, 10,  5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
    -5,  0,  0,  0,  0,  0,  0, -5,
     0,  0,  0,  5,  5,  0,  0,  0,
];

#[rustfmt::skip]
const QUEEN: [i32; 64] = [
   -20,-10,-10, -5, -5,-10,-10,-20,
   -10,  0,  0,  0,  0,  0,  0,-10,
   -10,  0,  5,  5,  5,  5,  0,-10,
    -5,  0,  5,  5,  5,  5,  0, -5,
     0,  0,  5,  5,  5,  5,  0, -5,
   -10,  5,  5,  5,  5,  5,  0,-10,
   -10,  0,  5,  0,  0,  0,  0,-10,
   -20,-10,-10, -5, -5,-10,-10,-20,
];

#[rustfmt::skip]
const KING_MIDDLEGAME: [i32; 64] = [
   -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30,
   -30,-40,-40,-50,-50,-40,-40,-30,
   -20,-30,-30,-40,-40,-30,-30,-20,
   -10,-20,-20,-20,-20,-20,-20,-10,
    20, 20,  0,  0,  0,  0, 20, 20,
    20, 30, 10,  0,  0, 10, 30, 20,
];

#[rustfmt::skip]
const KING_ENDGAME: [i32; 64] = [
   -50,-40,-30,-20,-20,-30,-40,-50,
   -30,-20,-10,  0,  0,-10,-20,-30,
   -30,-10, 20, 30, 30, 20,-10,-30,
   -30,-10, 30, 40, 40, 30,-10,-30,
   -30,-10, 30, 40, 40, 30,-10,-30,
   -30,-10, 20, 30, 30, 20,-10,-30,
   -30,-30,  0,  0,  0,  0,-30,-30,
   -50,-30,-30,-30,-30,-30,-30,-50,
];

//Returns Piece-Value Table for piece
fn table(pt: PieceType, endgame: bool)->&'static [i32; 64]{
    match pt {
        PieceType::Pawn => &PAWN,
        PieceType::Knight => &KNIGHT,
        PieceType::Bishop => &BISHOP,
        PieceType::Rook => &ROOK,
        PieceType::Queen => &QUEEN,
        PieceType::King => if endgame { &KING_ENDGAME } else { &KING_MIDDLEGAME },
    }
}

//PAWNSTRUCTURE
fn pawn_structure(board:&Board,color:Color)->Score{
    let mut pawns = board.piece_bitboard(color, PieceType::Pawn);
    let own = pawns;
    let enemy = board.piece_bitboard(color.opposite(), PieceType::Pawn);
    let mut score = Score::ZERO;

    for file in 0..8u8{
        let n = (own & Bitboard::file_mask(file)).count() as i32;
        if n > 1{
            score = score - Score::new(DOUBLED.mg * (n - 1), DOUBLED.eg * (n - 1));
        }
    }

    while let Some(sq) = pawns.pop_least_significant_bit(){
        let (file, rank) = (sq.file(), sq.rank());
        let adjacent = Bitboard::adjacent_files(file);
        if (own & adjacent).is_empty(){
            score = score - ISOLATED;
        }

        let ahead = (Bitboard::file_mask(file) | adjacent) & Bitboard::front_ranks(color, rank);
        if (enemy & ahead).is_empty(){
            let adv = (if color == Color::White { rank } else { 7 - rank }) as usize;
            score += Score::new(PASSED_MG[adv], PASSED_EG[adv]);
        }
        if !(own & adjacent & Bitboard::rank_behind(color, rank)).is_empty(){
            score += PROTECTED;
        }
    }
    score
}

//KINGSAFETY
//King ring plus the rank in front of it
fn king_zone(color:Color,ksq:Square)->Bitboard{
    let ring = king_attacks(ksq) | Bitboard::from_square(ksq);
    let forward = if color == Color::White { ring.north() } else { ring.south() };
    ring | forward
}

//Evaluates enemy attacks against the king zone
fn zone_attack_danger(board:&Board,color:Color,zone:Bitboard)->i32{
    let enemy = color.opposite();
    let occ = board.all_pieces;
    let (mut attackers, mut danger) = (0, 0);

    for (piecetype, weight) in [(PieceType::Knight, 2), (PieceType::Bishop, 2),
                         (PieceType::Rook, 3), (PieceType::Queen, 5)]{
        let mut bb = board.piece_bitboard(enemy, piecetype);
        while let Some(sq) = bb.pop_least_significant_bit(){
            let attacks = match piecetype{
                PieceType::Knight => knight_attacks(sq),
                PieceType::Bishop => bishop_attacks(sq, occ),
                PieceType::Rook => rook_attacks(sq, occ),
                _ => queen_attacks(sq, occ),
            };
            let hits = (attacks & zone).count() as i32;
            if hits > 0{
                attackers += 1;
                danger += weight * hits;
            }
        }
    }
    if attackers >= 2 { danger * 2 } else { danger }
}

//Returns the two ranks immediately ahead of the given rank
fn shield_ranks(color:Color,rank:u8)->Bitboard{
    let two_ahead = match color{
        Color::White => rank + 2,
        Color::Black => rank.saturating_sub(2),
    };
    Bitboard::front_ranks(color, rank) & !Bitboard::front_ranks(color, two_ahead)
}
//Evaluates king safety using pawn cover, open files, and enemy attacks
fn king_safety(board:&Board,color:Color)->Score{
    let Some(ksq) = board.king_square(color) else { return Score::ZERO };
    let own = board.piece_bitboard(color, PieceType::Pawn);
    let enemy_pawns = board.piece_bitboard(color.opposite(), PieceType::Pawn);
    let (file, rank) = (ksq.file(), ksq.rank());

    //Pawn shield
    let zone_files = Bitboard::file_mask(file) | Bitboard::adjacent_files(file);
    let shield = (own & zone_files & shield_ranks(color, rank)).count() as i32;
    let mut score = SHIELD_BONUS * shield.min(3);

    //Open and semi-open files next to the king
    for f in file.saturating_sub(1)..=(file + 1).min(7){
        let fm = Bitboard::file_mask(f);
        if (own & fm).is_empty(){
            score -= SEMI_OPEN_PENALTY;
            if (enemy_pawns & fm).is_empty() { score -= OPEN_EXTRA_PENALTY; }
        }
    }

    //Enemy attacks into the king zone
    score -= zone_attack_danger(board, color, king_zone(color, ksq));

    Score::new(score, 0)
}

//PIECEACTIVITY
//Returns the mobility baseline and middlegame/endgame weights for a piece type
fn mobility_weights(piecetype:PieceType)->(i32,i32,i32){
    match piecetype{
        PieceType::Knight => (4, 4, 4),
        PieceType::Bishop => (6, 5, 5),
        PieceType::Rook => (7, 2, 4),
        _ => (14, 1, 2),
    }
}

//Evaluates piece activity
fn piece_activity(board:&Board,color:Color)->Score{
    let own_pieces = board.all_pieces_by_color[color as usize];
    let occ = board.all_pieces;
    let own_pawns = board.piece_bitboard(color, PieceType::Pawn);
    let enemy_pawns = board.piece_bitboard(color.opposite(), PieceType::Pawn);
    let enemy_king = board.piece_bitboard(color.opposite(), PieceType::King);
    let (seventh, enemy_back) = if color == Color::White{
        (Bitboard::RANK_7, Bitboard::RANK_8)
    } else {
        (Bitboard::RANK_2, Bitboard::RANK_1)
    };
    let mut score = Score::ZERO;

    for piecetype in [PieceType::Knight, PieceType::Bishop, PieceType::Rook, PieceType::Queen]{
        let mut bb = board.piece_bitboard(color, piecetype);
        while let Some(sq) = bb.pop_least_significant_bit(){
            //1. Mobility
            let attacks = match piecetype{
                PieceType::Knight => knight_attacks(sq),
                PieceType::Bishop => bishop_attacks(sq, occ),
                PieceType::Rook => rook_attacks(sq, occ),
                _ => queen_attacks(sq, occ),
            };
            let moves = (attacks & !own_pieces).count() as i32;
            let (base, mg, eg) = mobility_weights(piecetype);
            score += Score::new((moves - base) * mg, (moves - base) * eg);

            if piecetype == PieceType::Rook{
                let file = Bitboard::file_mask(sq.file());
                if (own_pawns & file).is_empty(){
                    score += if (enemy_pawns & file).is_empty() { ROOK_OPEN } else { ROOK_SEMI_OPEN };
                }
                if seventh.contains(sq)
                    && (!(enemy_pawns & seventh).is_empty() || !(enemy_king & enemy_back).is_empty())
                {
                    score += ROOK_ON_SEVENTH;
                }
            }

            if piecetype == PieceType::Knight{
                let rel_rank = if color == Color::White { sq.rank() } else { 7 - sq.rank() };
                let protected = !(pawn_attacks(color.opposite(), sq) & own_pawns).is_empty();
                let ahead = Bitboard::adjacent_files(sq.file()) & Bitboard::front_ranks(color, sq.rank());
                if (3..=5).contains(&rel_rank) && protected && (enemy_pawns & ahead).is_empty() {
                    score += KNIGHT_OUTPOST;
                }
            }
        }
    }
    score
}

//TEMPO AND DRAWS
//Check if its draw via insufficient material
pub fn is_insufficient_material(board:&Board)->bool{
    let mut knights = 0;
    let mut bishops = Bitboard::EMPTY;
    for color in [Color::White, Color::Black]{
        if !board.piece_bitboard(color, PieceType::Pawn).is_empty()
            || !board.piece_bitboard(color, PieceType::Rook).is_empty()
            || !board.piece_bitboard(color, PieceType::Queen).is_empty()
        {
            return false;
        }
        knights += board.piece_bitboard(color, PieceType::Knight).count();
        bishops |= board.piece_bitboard(color, PieceType::Bishop);
    }

    if knights == 0{
        let dark = bishops & Bitboard(DARK_SQUARES);
        let light = bishops & !Bitboard(DARK_SQUARES);
        return dark.is_empty() || light.is_empty();
    }
    knights == 1 && bishops.is_empty()
}

//Check if only opposite bishops
fn opposite_bishops_only(board:&Board)->bool{
    for color in [Color::White, Color::Black]{
        if !board.piece_bitboard(color, PieceType::Knight).is_empty()
            || !board.piece_bitboard(color, PieceType::Rook).is_empty()
            || !board.piece_bitboard(color, PieceType::Queen).is_empty()
            || board.piece_bitboard(color, PieceType::Bishop).count() != 1
        {
            return false;
        }
    }
    let white = board.piece_bitboard(Color::White, PieceType::Bishop);
    let black = board.piece_bitboard(Color::Black, PieceType::Bishop);
    let dark = Bitboard(DARK_SQUARES);
    (white & dark).is_empty() != (black & dark).is_empty()
}

//SCORING/EVALUATION
//Calculates the game phase from remaining pieces
fn game_phase(board:&Board)->i32{
    let mut phase = 0;
    for color in [Color::White, Color::Black]{
        phase += board.piece_bitboard(color, PieceType::Knight).count() as i32;
        phase += board.piece_bitboard(color, PieceType::Bishop).count() as i32;
        phase += 2 * board.piece_bitboard(color, PieceType::Rook).count() as i32;
        phase += 4 * board.piece_bitboard(color, PieceType::Queen).count() as i32;
    }
    phase.min(MAX_PHASE)
}

//Material + piece-square score for one color
fn material_and_pst(board:&Board,color: Color)->Score{
    let mut score = Score::ZERO;
    for pt in PieceType::ALL{
        let (mg_tbl, eg_tbl) = (table(pt, false), table(pt, true));
        let mut bb = board.piece_bitboard(color, pt);
        while let Some(sq) = bb.pop_least_significant_bit(){
            let idx = if color == Color::White { sq.0 as usize ^ 56 } else { sq.0 as usize };
            let v = piece_value(pt);
            score += Score::new(v + mg_tbl[idx], v + eg_tbl[idx]);
        }
    }
    score
}

//Evaluates score for for the given side
fn side_score(board:&Board,color:Color)->Score{
    material_and_pst(board, color)
        + pawn_structure(board, color)
        + king_safety(board, color)
        + piece_activity(board, color)
}

//Evaluates position
pub fn evaluate(board:&Board)->i32{
    if is_insufficient_material(board){
        return 0;
    }

    let phase = game_phase(board);
    let us = board.side_to_move;
    let s = side_score(board, us) - side_score(board, us.opposite());
    let mut score = (s.mg * phase + s.eg * (MAX_PHASE - phase)) / MAX_PHASE;

    if opposite_bishops_only(board){
        score /= 2;
    }
    score + TEMPO
}