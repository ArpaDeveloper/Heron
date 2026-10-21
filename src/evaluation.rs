use crate::board::{Board, Color, PieceType};

//PIECEVALUE (centipawn)
fn piece_value(pt: PieceType)->i32{
    match pt {
        PieceType::Pawn => 100,
        PieceType::Knight => 320,
        PieceType::Bishop => 330,
        PieceType::Rook => 500,
        PieceType::Queen => 900,
        PieceType::King => 0,
    }
}

//Calculates total material value for a color
fn material(board:&Board, color:Color)->i32{
    PieceType::ALL
        .iter()
        .map(|&pt| board.piece_bitboard(color, pt).count() as i32 * piece_value(pt))
        .sum()
}

//Evaluates position by black's material - white's material
pub fn evaluate(board:&Board)->i32{
    let us = board.side_to_move;
    material(board, us)-material(board,us.opposite())
}











// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------
// - evaluate(board) -> i32 score in centipawns (100 = one pawn), always from
//   the point of view of the SIDE TO MOVE (positive = good for the player
//   about to move). Search relies on this convention for negamax.
// - It must be fast: it is called at almost every leaf of the search.
//   Prefer bitboard operations and popcounts over looping over squares.
//
// ---------------------------------------------------------------------------
// 1. Material
// ---------------------------------------------------------------------------
// - Piece values (starting point): pawn 100, knight 320, bishop 330,
//   rook 500, queen 900. The king has no material value.
// - Count with popcount on each piece bitboard, subtract Black from White.
// - Bishop pair bonus.
//
// ---------------------------------------------------------------------------
// 2. Piece-square tables
// ---------------------------------------------------------------------------
// - One 64-entry table per piece type giving a bonus or penalty per square
//   (knights toward the center, rooks on open files / 7th rank, pawns
//   advanced, king safe in the corner).
// - Tables are written from White's point of view; mirror the square
//   vertically for Black.
// - King needs two tables: middlegame (stay castled, behind pawns) and
//   endgame (go to the center).
//
// ---------------------------------------------------------------------------
// 3. Game phase and tapered evaluation
// ---------------------------------------------------------------------------
// - Compute a phase from the remaining non-pawn material (for example
//   knight/bishop = 1, rook = 2, queen = 4, total 24 at the start).
// - Keep a middlegame score and an endgame score for terms that differ, and
//   blend: (mg * phase + eg * (24 - phase)) / 24.
//
// ---------------------------------------------------------------------------
// 4. Pawn structure
// ---------------------------------------------------------------------------
// - Doubled pawns: penalty for several pawns on the same file.
// - Isolated pawns: penalty for no friendly pawns on adjacent files.
// - Passed pawns: bonus growing with the rank, larger in the endgame.
// - Optional: backward pawns, pawn chains, connected pawns.
// - Optional: a pawn hash table to cache the pawn evaluation.
//
// ---------------------------------------------------------------------------
// 5. Piece activity
// ---------------------------------------------------------------------------
// - Mobility: number of squares each knight, bishop, rook, queen can reach
//   (use the attack functions from movegen); small bonus per square.
// - Rooks on open and semi-open files, rook on the 7th rank.
// - Knight outposts (protected by a pawn, cannot be attacked by enemy pawns).
//
// ---------------------------------------------------------------------------
// 6. King safety
// ---------------------------------------------------------------------------
// - Pawn shield in front of the castled king.
// - Penalty for open files near the king.
// - Penalty based on the number of enemy attacks into the king zone.
// - Mostly a middlegame term: scale down as the phase approaches the endgame.
//
// ---------------------------------------------------------------------------
// 7. Tempo and draws
// ---------------------------------------------------------------------------
// - Small bonus for the side to move.
// - Recognize insufficient material (K vs K, K+minor vs K) and return 0.
// - Optional: scale the score down in known drawish endgames
//   (opposite-colored bishops).
//
// ---------------------------------------------------------------------------
// Tuning and testing
// ---------------------------------------------------------------------------
// - Keep every weight in one place (constants or a struct) so it can be
//   tuned later.
// - Sanity tests: the start position scores about 0 (plus tempo), the
//   evaluation is symmetric (a position and its color-flipped mirror give
//   the same score), and an extra queen is clearly winning.
// - Measure strength changes by playing matches against the previous
//   version, not by intuition.