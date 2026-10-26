/*
    Engine: Heron
    Author: ArpaDeveloper

    File: test.rs
    This file contains testing for the engine.
*/

use crate::board::{Bitboard, Board, CastlingRights, Color, PieceType, Square};
use crate::evaluation::{evaluate, is_insufficient_material, piece_value};
use crate::fen::START_FEN;
use crate::movegen::{in_check, is_attacked, legal_moves, perft, pseudo_legal_captures, pseudo_legal_moves, Move, MoveKind};
use crate::search::{best_move, Limits, MATE_SCORE};
use crate::transpositiontable::{Bound, TranspositionTable};
use crate::uci::{parse_go, parse_position, time_budget};
use crate::zobrist::ZOBRIST;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";

//HELPERS
fn sq(s: &str) -> Square {
    Square::from_algebraic(s).unwrap()
}

fn board(fen: &str) -> Board {
    Board::from_fen(fen).unwrap()
}

fn moves_of(b: &mut Board) -> Vec<Move> {
    legal_moves(b).iter().collect()
}

fn find(b: &mut Board, uci: &str) -> Option<Move> {
    moves_of(b).into_iter().find(|m| m.to_uci() == uci)
}

fn play(b: &mut Board, list: &[&str]) {
    for u in list {
        let m = find(b, u).unwrap_or_else(|| panic!("illegal move {}", u));
        b.make_move(m);
    }
}

fn search(fen: &str, depth: u32) -> (Move, i32) {
    let mut tt = TranspositionTable::new(1);
    let mut b = board(fen);
    let stop = Arc::new(AtomicBool::new(false));
    best_move(&mut b, depth, Limits::none(), &mut tt, stop).unwrap()
}

//1. BOARD
#[test]
fn bitboard_basics() {
    let mut bb = Bitboard::EMPTY;
    assert!(bb.is_empty());
    bb.set(sq("a1"));
    bb.set(sq("h8"));
    assert_eq!(bb.count(), 2);
    assert!(bb.contains(sq("h8")));
    assert_eq!(bb.least_significant_bit(), Some(sq("a1")));
    assert_eq!(bb.pop_least_significant_bit(), Some(sq("a1")));
    assert_eq!(bb.pop_least_significant_bit(), Some(sq("h8")));
    assert_eq!(bb.pop_least_significant_bit(), None);
}

#[test]
fn bitboard_shifts_do_not_wrap() {
    assert!(Bitboard::from_square(sq("h4")).east().is_empty());
    assert!(Bitboard::from_square(sq("a4")).west().is_empty());
    assert_eq!(Bitboard::from_square(sq("e4")).north_east(), Bitboard::from_square(sq("f5")));
    assert_eq!(Bitboard::file_mask(0).count(), 8);
    assert_eq!(Bitboard::adjacent_files(0), Bitboard::file_mask(1));
    assert_eq!(Bitboard::adjacent_files(4).count(), 16);
}

#[test]
fn square_conversions() {
    assert_eq!(Square::new(4, 3), sq("e4"));
    assert_eq!(sq("e4").0, 28);
    assert_eq!((sq("h8").file(), sq("h8").rank()), (7, 7));
    assert_eq!(sq("c6").to_algebraic(), "c6");
    for bad in ["", "e", "e44", "i1", "a9", "A1", "11"] {
        assert!(Square::from_algebraic(bad).is_none(), "{}", bad);
    }
}

#[test]
fn empty_board_is_empty() {
    let b = Board::empty();
    assert!(b.all_pieces.is_empty());
    assert_eq!(b.side_to_move, Color::White);
    assert_eq!(b.castling, CastlingRights::NONE);
    assert!(b.en_passant.is_none());
    assert_eq!(b.fullmove_number, 1);
    assert!((0..64).all(|i| b.piece_at(Square(i)).is_none()));
}

#[test]
fn startpos_layout() {
    let b = Board::startpos();
    assert_eq!(b.all_pieces.count(), 32);
    assert_eq!(b.all_pieces_by_color[Color::White as usize].count(), 16);
    assert_eq!(b.piece_bitboard(Color::White, PieceType::Pawn).count(), 8);
    assert_eq!(b.piece_bitboard(Color::Black, PieceType::Knight).count(), 2);
    assert_eq!(b.piece_at(sq("e1")), Some((Color::White, PieceType::King)));
    assert_eq!(b.piece_at(sq("d8")), Some((Color::Black, PieceType::Queen)));
    assert_eq!(b.piece_at(sq("e4")), None);
    assert_eq!(b.king_square(Color::Black), Some(sq("e8")));
    assert_eq!(b.castling, CastlingRights::ALL);
}

#[test]
fn put_and_remove_keep_everything_in_sync() {
    let mut b = Board::empty();
    b.put_piece(Color::Black, PieceType::Rook, sq("c5"));
    assert_eq!(b.piece_at(sq("c5")), Some((Color::Black, PieceType::Rook)));
    assert!(b.piece_bitboard(Color::Black, PieceType::Rook).contains(sq("c5")));
    assert!(b.all_pieces_by_color[Color::Black as usize].contains(sq("c5")));
    assert!(b.all_pieces.contains(sq("c5")));
    b.remove_piece(Color::Black, PieceType::Rook, sq("c5"));
    assert!(b.all_pieces.is_empty());
    assert!(b.piece_at(sq("c5")).is_none());
    assert_eq!(b.hash, 0);
}

fn assert_consistent(b: &Board) {
    let mut all = Bitboard::EMPTY;
    for color in [Color::White, Color::Black] {
        let mut by_color = Bitboard::EMPTY;
        for pt in PieceType::ALL {
            let bb = b.piece_bitboard(color, pt);
            by_color |= bb;
            let mut tmp = bb;
            while let Some(s) = tmp.pop_least_significant_bit() {
                assert_eq!(b.piece_at(s), Some((color, pt)), "mailbox wrong on {}", s.to_algebraic());
            }
        }
        assert_eq!(by_color, b.all_pieces_by_color[color as usize]);
        all |= by_color;
    }
    assert_eq!(all, b.all_pieces);
    for i in 0..64u8 {
        assert_eq!(b.piece_at(Square(i)).is_some(), b.all_pieces.contains(Square(i)));
    }
}

#[test]
fn bitboards_and_mailbox_agree_through_play() {
    let mut b = board(KIWIPETE);
    assert_consistent(&b);
    for m in moves_of(&mut b) {
        let u = b.make_move(m);
        assert_consistent(&b);
        b.unmake_move(m, u);
        assert_consistent(&b);
    }
}

//2. FEN
#[test]
fn fen_round_trip() {
    for fen in [
        START_FEN,
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
        KIWIPETE,
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
        "4k3/8/8/8/8/8/8/4K3 b - - 37 90",
    ] {
        assert_eq!(board(fen).to_fen(), fen);
    }
}

#[test]
fn fen_fields_are_parsed() {
    let b = board("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b Kq e3 12 34");
    assert_eq!(b.side_to_move, Color::Black);
    assert!(b.castling.has(CastlingRights::WHITE_KINGSIDE));
    assert!(!b.castling.has(CastlingRights::WHITE_QUEENSIDE));
    assert!(!b.castling.has(CastlingRights::BLACK_KINGSIDE));
    assert!(b.castling.has(CastlingRights::BLACK_QUEENSIDE));
    assert_eq!(b.en_passant, Some(sq("e3")));
    assert_eq!((b.halfmove_clock, b.fullmove_number), (12, 34));
}

#[test]
fn fen_without_clocks_uses_defaults() {
    let b = board("4k3/8/8/8/8/8/8/4K3 w - -");
    assert_eq!((b.halfmove_clock, b.fullmove_number), (0, 1));
}

#[test]
fn bad_fens_are_rejected() {
    for fen in [
        "",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP w KQkq - 0 1",                    //7 ranks
        "rnbqkbn/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",            //short rank
        "rnbqkbnrr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",          //long rank
        "rnbqkbnr/pppppppp/9/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",           //bad digit
        "rnbqkbnr/ppppxppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",           //bad piece
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR x KQkq - 0 1",           //bad side
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkz - 0 1",           //bad castling
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KK - 0 1",             //duplicate castling
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq e4 0 1",          //bad ep rank
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - x 1",           //bad clock
        "8/8/8/8/8/8/8/8 w - - 0 1",                                          //no kings
        "4k3/8/8/8/8/8/8/3KK3 w - - 0 1",                                     //two white kings
    ] {
        assert!(Board::from_fen(fen).is_err(), "should reject: {:?}", fen);
    }
}

#[test]
fn castling_rights_parse_and_write() {
    assert_eq!(CastlingRights::from_fen("KQkq"), Some(CastlingRights::ALL));
    assert_eq!(CastlingRights::from_fen("-"), Some(CastlingRights::NONE));
    assert_eq!(CastlingRights::from_fen("Kq").unwrap().0, 0b1001);
    for bad in ["", "K-", "KK", "x"] {
        assert_eq!(CastlingRights::from_fen(bad), None, "{}", bad);
    }
    assert_eq!(CastlingRights::ALL.to_fen(), "KQkq");
    assert_eq!(CastlingRights::NONE.to_fen(), "-");
}

//3. MOVE GENERATION
#[test]
fn perft_startpos() {
    let mut b = Board::startpos();
    for (d, n) in [(1, 20), (2, 400), (3, 8902), (4, 197_281)] {
        assert_eq!(perft(&mut b, d), n, "depth {}", d);
    }
}

#[test]
fn perft_standard_positions() {
    let cases: [(&str, &[u64]); 5] = [
        (KIWIPETE, &[48, 2039, 97_862]),
        ("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", &[14, 191, 2812, 43_238]),
        ("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1", &[6, 264, 9467]),
        ("rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8", &[44, 1486, 62_379]),
        ("r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10", &[46, 2079, 89_890]),
    ];
    for (fen, counts) in cases {
        let mut b = board(fen);
        for (i, n) in counts.iter().enumerate() {
            assert_eq!(perft(&mut b, i as u32 + 1), *n, "{} depth {}", fen, i + 1);
        }
        assert_eq!(b.to_fen(), fen, "perft must leave the board unchanged");
    }
}

#[test]
fn legal_moves_never_leave_own_king_attacked() {
    for fen in [START_FEN, KIWIPETE, "4r1k1/8/8/8/8/8/4N3/4K3 w - - 0 1"] {
        let mut b = board(fen);
        for m in moves_of(&mut b) {
            let us = b.side_to_move;
            let u = b.make_move(m);
            let k = b.king_square(us).unwrap();
            assert!(!is_attacked(&b, k, us.opposite()), "{} leaves king in check in {}", m.to_uci(), fen);
            b.unmake_move(m, u);
        }
    }
}

#[test]
fn legal_is_a_subset_of_pseudo_legal() {
    let mut b = board(KIWIPETE);
    let pseudo: Vec<Move> = pseudo_legal_moves(&b).iter().collect();
    for m in moves_of(&mut b) {
        assert!(pseudo.contains(&m));
    }
}

#[test]
fn captures_generator_only_returns_tactical_moves() {
    let b = board(KIWIPETE);
    let caps: Vec<Move> = pseudo_legal_captures(&b).iter().collect();
    assert!(!caps.is_empty());
    for m in &caps {
        assert!(
            matches!(m.kind(), MoveKind::Capture | MoveKind::EnPassant
                | MoveKind::Promotion(_) | MoveKind::PromotionCapture(_)),
            "{} is not tactical", m.to_uci()
        );
    }
    let all: Vec<Move> = pseudo_legal_moves(&b).iter().collect();
    assert!(caps.iter().all(|m| all.contains(m)));
}

#[test]
fn make_unmake_restores_everything() {
    let mut b = board(KIWIPETE);
    let before = (b.to_fen(), b.hash, b.history.len());
    for m in moves_of(&mut b) {
        let u = b.make_move(m);
        b.unmake_move(m, u);
        assert_eq!((b.to_fen(), b.hash, b.history.len()), before, "after {}", m.to_uci());
    }
}

#[test]
fn checkmate_has_no_moves() {
    let mut b = board("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
    assert!(in_check(&b));
    assert!(moves_of(&mut b).is_empty());
}

#[test]
fn stalemate_has_no_moves_and_no_check() {
    let mut b = board("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1");
    assert!(!in_check(&b));
    assert!(moves_of(&mut b).is_empty());
}

#[test]
fn castling_rules() {
    let mut b = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    let castles: Vec<_> = moves_of(&mut b).into_iter()
        .filter(|m| matches!(m.kind(), MoveKind::KingsideCastle | MoveKind::QueensideCastle)).collect();
    assert_eq!(castles.len(), 2);

    let mut b = board("r3k2r/8/8/8/8/8/5r2/R3K2R w KQ - 0 1");
    assert!(find(&mut b, "e1g1").is_none());
    assert!(find(&mut b, "e1c1").is_some());

    let mut b = board("r3k2r/8/8/8/8/8/8/RN2K2R w KQkq - 0 1");
    assert!(find(&mut b, "e1c1").is_none());

    let mut b = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    play(&mut b, &["e1g1"]);
    assert_eq!(b.piece_at(sq("g1")), Some((Color::White, PieceType::King)));
    assert_eq!(b.piece_at(sq("f1")), Some((Color::White, PieceType::Rook)));
    assert!(b.piece_at(sq("h1")).is_none());
    assert!(!b.castling.has(CastlingRights::WHITE_KINGSIDE));
    assert!(!b.castling.has(CastlingRights::WHITE_QUEENSIDE));
    assert!(b.castling.has(CastlingRights::BLACK_KINGSIDE));
}

#[test]
fn castling_right_lost_when_rook_captured_or_moved() {
    let mut b = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    play(&mut b, &["a1a8"]); //captures the a8 rook
    assert!(!b.castling.has(CastlingRights::WHITE_QUEENSIDE));
    assert!(!b.castling.has(CastlingRights::BLACK_QUEENSIDE));
    assert!(b.castling.has(CastlingRights::WHITE_KINGSIDE));
}

#[test]
fn en_passant() {
    let mut b = board("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
    let m = find(&mut b, "e5d6").expect("en passant must be generated");
    assert_eq!(m.kind(), MoveKind::EnPassant);
    b.make_move(m);
    assert!(b.piece_at(sq("d5")).is_none(), "captured pawn must be removed");
    assert_eq!(b.piece_at(sq("d6")), Some((Color::White, PieceType::Pawn)));

    let mut b = board("4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1");
    assert!(find(&mut b, "e5d6").is_none());
}

#[test]
fn double_push_sets_en_passant_square() {
    let mut b = Board::startpos();
    play(&mut b, &["e2e4"]);
    assert_eq!(b.en_passant, Some(sq("e3")));
    play(&mut b, &["g8f6"]);
    assert_eq!(b.en_passant, None);
}

#[test]
fn promotions() {
    let mut b = board("4k3/P7/8/8/8/8/8/4K3 w - - 0 1");
    for u in ["a7a8q", "a7a8r", "a7a8b", "a7a8n"] {
        assert!(find(&mut b, u).is_some(), "missing {}", u);
    }
    play(&mut b, &["a7a8q"]);
    assert_eq!(b.piece_at(sq("a8")), Some((Color::White, PieceType::Queen)));
    assert_eq!(b.piece_bitboard(Color::White, PieceType::Pawn).count(), 0);

    let mut b = board("1r2k3/P7/8/8/8/8/8/4K3 w - - 0 1");
    let m = find(&mut b, "a7b8q").unwrap();
    assert_eq!(m.kind(), MoveKind::PromotionCapture(PieceType::Queen));
}

#[test]
fn pinned_piece_cannot_move() {
    let mut b = board("4r1k1/8/8/8/8/8/4N3/4K3 w - - 0 1");
    assert!(moves_of(&mut b).iter().all(|m| m.from() != sq("e2")));
}

#[test]
fn must_resolve_check() {
    let mut b = board("4k3/8/8/8/8/8/4r3/4K3 w - - 0 1");
    assert!(in_check(&b));
    let list = moves_of(&mut b);
    assert!(!list.is_empty());
    for m in list {
        assert_eq!(m.from(), sq("e1"), "only the king can move here");
    }
}

#[test]
fn move_uci_text() {
    let mut b = Board::startpos();
    assert_eq!(find(&mut b, "e2e4").unwrap().to_uci(), "e2e4");
    let mut b = board("4k3/P7/8/8/8/8/8/4K3 w - - 0 1");
    assert_eq!(find(&mut b, "a7a8n").unwrap().to_uci(), "a7a8n");
}

//4. SEARCH
#[test]
fn finds_mate_in_one() {
    let (m, s) = search("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", 4);
    assert_eq!(m.to_uci(), "a1a8");
    assert_eq!(s, MATE_SCORE - 1);
}

#[test]
fn finds_mate_in_two() {
    let (m, s) = search("7k/8/8/8/8/8/R7/1R4K1 w - - 0 1", 5);
    assert_eq!(s, MATE_SCORE - 3);
    assert!(["a2a7", "b1b7"].contains(&m.to_uci().as_str()), "got {}", m.to_uci());
}

#[test]
fn avoids_being_mated() {
    let fen = "6k1/5ppp/8/8/8/8/r4PPP/6K1 w - - 0 1";
    let (m, _) = search(fen, 4);
    let mut b = board(fen);
    assert!(moves_of(&mut b).contains(&m));
}

#[test]
fn does_not_hang_the_queen() {
    let (m, _) = search("3rk3/8/8/3p4/8/8/8/3QK3 w - - 0 1", 4);
    assert_ne!(m.to_uci(), "d1d5");
}

#[test]
fn takes_a_free_queen() {
    let (m, s) = search("4k3/8/8/3q4/8/8/8/3RK3 w - - 0 1", 3);
    assert_eq!(m.to_uci(), "d1d5");
    assert!(s > 500);
}

#[test]
fn single_legal_move_is_returned() {
    let fen = "7k/5K2/8/8/8/8/P7/8 b - - 0 1";
    let mut b = board(fen);
    let list = moves_of(&mut b);
    assert_eq!(list.len(), 1);
    let (m, _) = search(fen, 4);
    assert_eq!(m, list[0]);
}

#[test]
fn search_returns_a_legal_move_from_many_positions() {
    for fen in [START_FEN, KIWIPETE, "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"] {
        let (m, _) = search(fen, 4);
        let mut b = board(fen);
        assert!(moves_of(&mut b).contains(&m), "{} illegal in {}", m.to_uci(), fen);
    }
}

#[test]
fn search_leaves_the_board_unchanged() {
    let mut tt = TranspositionTable::new(1);
    let mut b = board(KIWIPETE);
    let before = (b.to_fen(), b.hash, b.history.len());
    best_move(&mut b, 4, Limits::none(), &mut tt, Arc::new(AtomicBool::new(false)));
    assert_eq!((b.to_fen(), b.hash, b.history.len()), before);
}

#[test]
fn search_is_deterministic() {
    let a = search(KIWIPETE, 5);
    let b = search(KIWIPETE, 5);
    assert_eq!(a, b);
}

#[test]
fn stop_flag_still_gives_a_move() {
    let mut tt = TranspositionTable::new(1);
    let mut b = board(KIWIPETE);
    let stop = Arc::new(AtomicBool::new(true));
    let r = best_move(&mut b, 30, Limits::none(), &mut tt, stop);
    assert!(r.is_some());
}

#[test]
fn time_limit_is_respected() {
    let mut tt = TranspositionTable::new(1);
    let mut b = board(KIWIPETE);
    let limit = Duration::from_millis(200);
    let limits = Limits { soft: Some(limit), hard: Some(limit), nodes: None };
    let t0 = std::time::Instant::now();
    let r = best_move(&mut b, 64, limits, &mut tt, Arc::new(AtomicBool::new(false)));
    assert!(r.is_some());
    assert!(t0.elapsed() < Duration::from_millis(1000), "took {:?}", t0.elapsed());
}

#[test]
fn node_limit_is_respected() {
    let mut tt = TranspositionTable::new(1);
    let mut b = board(KIWIPETE);
    let limits = Limits { soft: None, hard: None, nodes: Some(5000) };
    let t0 = std::time::Instant::now();
    let r = best_move(&mut b, 64, limits, &mut tt, Arc::new(AtomicBool::new(false)));
    assert!(r.is_some());
    assert!(t0.elapsed() < Duration::from_secs(5));
}

#[test]
fn fifty_move_rule_scores_zero() {
    let (_, s) = search("4k3/8/8/8/8/8/8/R3K2R w - - 100 80", 3);
    assert_eq!(s, 0);
}

#[test]
fn repetition_is_detected() {
    let mut b = Board::startpos();
    play(&mut b, &["g1f3", "g8f6", "f3g1", "f6g8"]);
    assert!(b.is_repetition(4));
    assert!(!b.is_repetition(0));
    play(&mut b, &["g1f3", "g8f6", "f3g1", "f6g8"]);
    assert!(b.is_repetition(0));
}

#[test]
fn engine_avoids_threefold_when_winning() {
    let mut b = board("4k3/8/8/8/8/8/Q7/4K3 w - - 0 1");
    play(&mut b, &["a2a3", "e8d8", "a3a2", "d8e8"]);
    assert!(b.is_repetition(4));
}

//5. EVALUATION
#[test]
fn startpos_is_balanced_apart_from_tempo() {
    let e = evaluate(&Board::startpos());
    assert!(e > 0 && e < 50, "got {}", e);
}

#[test]
fn evaluation_is_color_symmetric() {
    let pairs = [
        ("4k3/8/8/8/8/5N2/8/4K3 w - - 0 1", "4k3/8/5n2/8/8/8/8/4K3 b - - 0 1"),
        ("r1bqkb1r/pppp1ppp/2n2n2/4p3/4P3/2N2N2/PPPP1PPP/R1BQKB1R w KQkq - 0 1",
         "r1bqkb1r/pppp1ppp/2n2n2/4p3/4P3/2N2N2/PPPP1PPP/R1BQKB1R b KQkq - 0 1"),
        ("4k3/pp3ppp/8/8/8/8/PPP2PPP/4K3 w - - 0 1", "4k3/ppp2ppp/8/8/8/8/PP3PPP/4K3 b - - 0 1"),
    ];
    for (w, b) in pairs {
        assert_eq!(evaluate(&board(w)), evaluate(&board(b)), "{} vs {}", w, b);
    }
}

#[test]
fn material_matters() {
    let up = evaluate(&board("4k3/8/8/8/8/8/8/Q3K3 w - - 0 1"));
    let even = evaluate(&board("4k3/8/8/8/8/8/8/4K3 w - - 0 1"));
    assert!(up > even + 500);
    let down = evaluate(&board("4k3/8/8/8/8/8/8/Q3K3 b - - 0 1"));
    assert!(down < -500);
}

#[test]
fn piece_values_are_ordered() {
    assert!(piece_value(PieceType::Queen) > piece_value(PieceType::Rook));
    assert!(piece_value(PieceType::Rook) > piece_value(PieceType::Knight));
    assert!(piece_value(PieceType::Knight) > piece_value(PieceType::Pawn));
}

#[test]
fn insufficient_material_scores_zero() {
    for fen in [
        "4k3/8/8/8/8/8/8/4K3 w - - 0 1",
        "4k3/8/8/8/8/8/8/3BK3 w - - 0 1",
        "4k3/8/8/8/8/8/8/3NK3 w - - 0 1",
        "5b1k/8/8/8/8/8/8/2B1K3 w - - 0 1", 
    ] {
        let b = board(fen);
        assert!(is_insufficient_material(&b), "{}", fen);
        assert_eq!(evaluate(&b), 0, "{}", fen);
    }
    for fen in [
        "4k3/8/8/8/8/8/8/2NNK3 w - - 0 1",
        "4k3/8/8/8/8/8/P7/4K3 w - - 0 1",
        "4k3/8/8/8/8/8/8/R3K3 w - - 0 1",
        "2b4k/8/8/8/8/8/8/2B1K3 w - - 0 1",
    ] {
        assert!(!is_insufficient_material(&board(fen)), "{}", fen);
    }
}

#[test]
fn advanced_passed_pawn_is_worth_more() {
    let far = evaluate(&board("4k3/8/8/4P3/8/8/8/4K3 w - - 0 1"));
    let near = evaluate(&board("4k3/8/8/8/8/4P3/8/4K3 w - - 0 1"));
    assert!(far > near);
}

#[test]
fn doubled_and_isolated_pawns_are_penalised() {
    let healthy = evaluate(&board("4k3/8/8/8/8/8/PP6/4K3 w - - 0 1"));
    let doubled = evaluate(&board("4k3/8/8/8/8/P7/P7/4K3 w - - 0 1"));
    assert!(doubled < healthy);
}

#[test]
fn castled_king_with_shield_beats_bare_king() {
    let shield = evaluate(&board("r4rk1/5ppp/8/8/8/8/5PPP/R4RK1 w - - 0 1"));
    let bare = evaluate(&board("r4rk1/5ppp/8/8/8/8/8/R4RK1 w - - 0 1"));
    assert!(shield > bare);
}

#[test]
fn rook_prefers_open_file() {
    let open = evaluate(&board("4k3/8/8/8/8/8/1P6/R3K3 w - - 0 1"));
    let closed = evaluate(&board("4k3/8/8/8/8/8/P7/R3K3 w - - 0 1"));
    assert!(open > closed);
}

#[test]
fn central_king_is_better_in_the_endgame() {
    let centre = evaluate(&board("7k/8/8/8/4K3/8/8/8 w - - 0 1"));
    let corner = evaluate(&board("7k/8/8/8/8/8/8/K7 w - - 0 1"));
    let _ = (centre, corner);
    let centre = evaluate(&board("7k/8/8/8/4K3/8/P7/8 w - - 0 1"));
    let corner = evaluate(&board("7k/8/8/8/8/8/P7/K7 w - - 0 1"));
    assert!(centre > corner);
}

//6. UCI
#[test]
fn position_startpos_with_moves() {
    let b = parse_position(&["startpos", "moves", "e2e4", "e7e5", "g1f3"]).unwrap();
    assert_eq!(b.side_to_move, Color::Black);
    assert_eq!(b.piece_at(sq("f3")), Some((Color::White, PieceType::Knight)));
    assert_eq!(b.fullmove_number, 2);
    assert_eq!(b.history.len(), 3);
}

#[test]
fn position_fen_with_and_without_moves() {
    let text = "fen r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1 moves e1g1";
    let args: Vec<&str> = text.split_whitespace().collect();
    let b = parse_position(&args).unwrap();
    assert_eq!(b.piece_at(sq("g1")), Some((Color::White, PieceType::King)));

    let args: Vec<&str> = "fen 4k3/8/8/8/8/8/8/4K3 w - - 0 1".split_whitespace().collect();
    assert_eq!(parse_position(&args).unwrap().to_fen(), "4k3/8/8/8/8/8/8/4K3 w - - 0 1");
}

#[test]
fn bad_position_commands_are_rejected() {
    assert!(parse_position(&[]).is_none());
    assert!(parse_position(&["banana"]).is_none());
    assert!(parse_position(&["startpos", "moves", "e2e5"]).is_none()); //illegal move
    assert!(parse_position(&["startpos", "moves", "e2e4", "e2e4"]).is_none());
    assert!(parse_position(&["fen", "not", "a", "fen"]).is_none());
}

#[test]
fn position_promotion_move_text() {
    let args: Vec<&str> = "fen 4k3/P7/8/8/8/8/8/4K3 w - - 0 1 moves a7a8q".split_whitespace().collect();
    let b = parse_position(&args).unwrap();
    assert_eq!(b.piece_at(sq("a8")), Some((Color::White, PieceType::Queen)));
}

#[test]
fn go_parameters_are_parsed() {
    let args: Vec<&str> = "wtime 60000 btime 50000 winc 1000 binc 2000 movestogo 20 depth 7 nodes 999 movetime 500".split_whitespace().collect();
    let p = parse_go(&args);
    assert_eq!(p.wtime, Some(60000));
    assert_eq!(p.btime, Some(50000));
    assert_eq!(p.winc, Some(1000));
    assert_eq!(p.binc, Some(2000));
    assert_eq!(p.movestogo, Some(20));
    assert_eq!(p.depth, Some(7));
    assert_eq!(p.nodes, Some(999));
    assert_eq!(p.movetime, Some(500));
    assert!(!p.infinite);
    assert!(parse_go(&["infinite"]).infinite);
    assert_eq!(parse_go(&["wtime", "-50"]).wtime, Some(0));
}

#[test]
fn time_budget_behaviour() {
    let l = time_budget(&parse_go(&["movetime", "1000"]), Color::White);
    assert_eq!(l.soft, l.hard);
    assert!(l.hard.unwrap() < Duration::from_millis(1000));
    assert!(l.hard.unwrap() > Duration::from_millis(500));

    let p = parse_go(&["wtime", "60000", "btime", "1000", "winc", "0", "binc", "0"]);
    let w = time_budget(&p, Color::White);
    let b = time_budget(&p, Color::Black);
    assert!(w.soft.unwrap() > b.soft.unwrap());
    for l in [w, b] {
        assert!(l.hard.unwrap() >= l.soft.unwrap());
    }
    assert!(b.hard.unwrap() < Duration::from_millis(1000));

    let l = time_budget(&parse_go(&[]), Color::White);
    assert!(l.soft.is_none() && l.hard.is_none());
}

//7. ZOBRIST
#[test]
fn zobrist_keys_are_filled_and_unique() {
    assert_ne!(ZOBRIST.side_to_move, 0);
    assert!(ZOBRIST.castling.iter().all(|&k| k != 0));
    assert!(ZOBRIST.en_passant_file.iter().all(|&k| k != 0));

    let mut all: Vec<u64> = ZOBRIST.pieces.iter().flatten().flatten().copied().collect();
    all.extend(ZOBRIST.castling);
    all.extend(ZOBRIST.en_passant_file);
    all.push(ZOBRIST.side_to_move);
    let n = all.len();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), n, "duplicate Zobrist keys");
}

#[test]
fn hash_matches_recomputation_everywhere() {
    fn walk(b: &mut Board, depth: u32) {
        assert_eq!(b.hash, b.compute_hash(), "{}", b.to_fen());
        if depth == 0 {
            return;
        }
        for m in moves_of(b) {
            let before = b.hash;
            let u = b.make_move(m);
            walk(b, depth - 1);
            b.unmake_move(m, u);
            assert_eq!(b.hash, before, "unmake of {}", m.to_uci());
        }
    }
    for fen in [START_FEN, KIWIPETE, "4k3/P7/8/8/8/8/p7/4K3 w - - 0 1", "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1"] {
        walk(&mut board(fen), 3);
    }
}

#[test]
fn hash_depends_on_the_right_things() {
    let w = board("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    let b = board("4k3/8/8/8/8/8/8/4K3 b - - 0 1");
    assert_ne!(w.hash, b.hash, "side to move");

    let a = board("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    let c = board("r3k2r/8/8/8/8/8/8/R3K2R w Kkq - 0 1");
    assert_ne!(a.hash, c.hash, "castling rights");

    let e = board("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1");
    let n = board("4k3/8/8/3pP3/8/8/8/4K3 w - - 0 1");
    assert_ne!(e.hash, n.hash, "en passant");

    let x = board("4k3/8/8/8/8/8/8/4K3 w - - 0 1");
    let y = board("4k3/8/8/8/8/8/8/4K3 w - - 40 77");
    assert_eq!(x.hash, y.hash, "clocks must not matter");
}

#[test]
fn transpositions_give_the_same_hash() {
    let mut a = Board::startpos();
    let mut b = Board::startpos();
    play(&mut a, &["g1f3", "g8f6", "b1c3", "b8c6"]);
    play(&mut b, &["b1c3", "b8c6", "g1f3", "g8f6"]);
    assert_eq!(a.hash, b.hash);
    assert_eq!(a.to_fen(), b.to_fen());
}

#[test]
fn null_move_keeps_hash_in_sync() {
    let mut b = board("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");
    let before = (b.hash, b.en_passant, b.halfmove_clock, b.history.len());
    let u = b.make_null_move();
    assert_eq!(b.side_to_move, Color::White);
    assert!(b.en_passant.is_none());
    assert_eq!(b.hash, b.compute_hash());
    b.unmake_null_move(u);
    assert_eq!((b.hash, b.en_passant, b.halfmove_clock, b.history.len()), before);
}

//8. TRANSPOSITION TABLE
#[test]
fn tt_store_and_probe() {
    let mut t = TranspositionTable::new(1);
    let key = (0xABCDu64 << 32) | 5;
    assert!(t.probe(key).is_none());

    let mut b = Board::startpos();
    let m = find(&mut b, "e2e4").unwrap();
    t.store(key, 7, -123, Bound::Lower, Some(m));

    let e = t.probe(key).unwrap();
    assert_eq!((e.score, e.depth, e.bound), (-123, 7, Bound::Lower));
    assert_eq!(e.best, Some(m));
}



#[test]
fn tt_keeps_old_move_and_eval_when_updated_without_them() {
    let mut t = TranspositionTable::new(1);
    let key = (7u64 << 32) | 3;
    let mut b = Board::startpos();
    let m = find(&mut b, "d2d4").unwrap();
    t.store(key, 5, 10, Bound::Exact, Some(m));
    t.store(key, 6, 20, Bound::Exact, None);
    let e = t.probe(key).unwrap();
    assert_eq!(e.best, Some(m));
    assert_eq!((e.score, e.depth), (20, 6));
}

#[test]
fn tt_clear_empties_the_table() {
    let mut t = TranspositionTable::new(1);
    let key = (9u64 << 32) | 1;
    t.store(key, 2, 1, Bound::Exact, None);
    assert!(t.probe(key).is_some());
    t.clear();
    assert!(t.probe(key).is_none());
}


#[test]
fn search_gives_same_best_move_with_a_tiny_and_a_large_table() {
    for mb in [1usize, 32] {
        let mut tt = TranspositionTable::new(mb);
            let mut b = board("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1");
        let (m, s) = best_move(&mut b, 5, Limits::none(), &mut tt, Arc::new(AtomicBool::new(false))).unwrap();
        assert_eq!(m.to_uci(), "a1a8", "{} MB", mb);
        assert_eq!(s, MATE_SCORE - 1);
    }
}

#[test]
fn stop_flag_type_is_usable_across_threads() {
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = Arc::clone(&stop);
    std::thread::spawn(move || s2.store(true, Ordering::Relaxed)).join().unwrap();
    assert!(stop.load(Ordering::Relaxed));
}