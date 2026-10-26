/*
    Engine: Heron
    Author: ArpaDeveloper

    File: movegen.rs
    This file handles generation of legal moves.
*/

use crate::board::{Bitboard, Board, CastlingRights, Color, PieceType, Square};

//MOVETYPE
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MoveKind{
    Quiet,
    DoublePawnPush,
    Capture,
    EnPassant,
    KingsideCastle,
    QueensideCastle,
    Promotion(PieceType),
    PromotionCapture(PieceType),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Move{
    from: Square,
    to: Square,
    kind: MoveKind,
}

impl Move{
    //Create a new Move
    fn new(from: Square, to: Square, kind: MoveKind) -> Move {
        Move { from, to, kind }
    }

    //Getters
    pub fn from(self)->Square{ self.from }
    pub fn to(self)->Square{ self.to }
    pub fn kind(self)->MoveKind{ self.kind }

    //Getter for promotion piece
    pub fn promotion_piece(self)->Option<PieceType>{
        match self.kind{
            MoveKind::Promotion(p) | MoveKind::PromotionCapture(p) => Some(p),
            _ => None,
        }
    }

    //Converter for move to UCI
    pub fn to_uci(self)->String{
        let mut s = format!("{}{}", self.from.to_algebraic(), self.to.to_algebraic());
        if let Some(p) = self.promotion_piece() {s.push(p.to_char());}
        s
    }
}

//MOVELIST
pub const MAX_MOVES: usize = 256;

#[derive(Clone)]
pub struct MoveList{
    moves: [Option<Move>; MAX_MOVES],
    len: usize,
}

impl MoveList{
    //Create new MoveList
    pub fn new()->MoveList{
        MoveList{
            moves: [None; MAX_MOVES],
            len: 0,
        }
    }

    //Adds move to the MoveList
    pub fn push(&mut self,m:Move){
        self.moves[self.len] = Some(m);
        self.len += 1;
    }

    //Getter for size of the MoveList
    pub fn len(&self)->usize{
        self.len
    }

    //Checks if the MoveList is empty
    pub fn is_empty(&self)->bool{
        self.len == 0
    }

    //Returns an iterator over the moves in the list
    pub fn iter(&self)->impl Iterator<Item = Move> + '_{
        self.moves[..self.len].iter().map(|m| m.expect("slots below len are always filled"))
    }

    //Getter for move at given index
    pub fn get(&self,i:usize)->Move{
        self.moves[i].expect("index below len")
    }

    //Swaps two moves in the list
    pub fn swap(&mut self,a:usize,b:usize){
        self.moves.swap(a, b);
    }
}

//ATTACKTABLES
//Returns the squares a knight can attack from given square
const fn knight_from(sq:Square)->Bitboard{
    let b = Bitboard::from_square(sq);
    Bitboard(
        b.north().north().east().0
            | b.north().north().west().0
            | b.north().east().east().0
            | b.north().west().west().0
            | b.south().south().east().0
            | b.south().south().west().0
            | b.south().east().east().0
            | b.south().west().west().0,
    )
}

//Returns the squares a king can attack from given square
const fn king_from(sq:Square)->Bitboard{
    let b = Bitboard::from_square(sq);
    Bitboard(
        b.north().0
            | b.south().0
            | b.east().0
            | b.west().0
            | b.north_east().0
            | b.north_west().0
            | b.south_east().0
            | b.south_west().0,
    )
}

//Returns the squares a pawn can attack from given square & color
const fn pawn_from(sq: Square,color: Color)->Bitboard{
    let b = Bitboard::from_square(sq);
    match color{
        Color::White => Bitboard(b.north_east().0 | b.north_west().0),
        Color::Black => Bitboard(b.south_east().0 | b.south_west().0),
    }
}

//Lookup table for knight attacks
const fn knight_table()->[Bitboard;64]{
    let mut t = [Bitboard::EMPTY; 64];
    let mut i = 0;
    while i < 64{
        t[i] = knight_from(Square(i as u8));
        i += 1;
    }
    t
}

//Lookup table for king attacks
const fn king_table()->[Bitboard;64]{
    let mut t = [Bitboard::EMPTY; 64];
    let mut i = 0;
    while i < 64{
        t[i] = king_from(Square(i as u8));
        i += 1;
    }
    t
}

//Lookup table for pawn attacks
const fn pawn_table(color:Color)->[Bitboard;64]{
    let mut t = [Bitboard::EMPTY; 64];
    let mut i = 0;
    while i < 64{
        t[i] = pawn_from(Square(i as u8), color);
        i += 1;
    }
    t
}

//Constants for knight, king and pawn attacks
const KNIGHT_ATTACKS: [Bitboard; 64] = knight_table();
const KING_ATTACKS: [Bitboard; 64] = king_table();
const PAWN_ATTACKS: [[Bitboard; 64]; 2] = [pawn_table(Color::White), pawn_table(Color::Black)];

//Getters for precomputated attacks on given square
pub fn knight_attacks(sq:Square)->Bitboard{
    KNIGHT_ATTACKS[sq.0 as usize]
}
pub fn king_attacks(sq:Square)->Bitboard{
    KING_ATTACKS[sq.0 as usize]
}
pub fn pawn_attacks(color:Color,sq:Square)->Bitboard{
    PAWN_ATTACKS[color as usize][sq.0 as usize]
}

//SLIDINGATTACKS (ray walking; can later be replaced with magic bitboards)
//Returns sliding attacks in one direction, stopping at the first blocker or board edge
fn ray_attacks(sq:Square,occ:Bitboard,step:fn(Bitboard)->Bitboard)->Bitboard{
    let mut attacks = Bitboard::EMPTY;
    let mut current = Bitboard::from_square(sq);
    loop{
        current = step(current);
        if current.is_empty(){
            break;
        }
        attacks |= current;
        if !(current & occ).is_empty() {
            break;
        }
    }
    attacks
}

//Returns all squares a rook attacks
pub fn rook_attacks(sq:Square,occ:Bitboard)->Bitboard{
    ray_attacks(sq, occ, Bitboard::north)
        | ray_attacks(sq, occ, Bitboard::south)
        | ray_attacks(sq, occ, Bitboard::east)
        | ray_attacks(sq, occ, Bitboard::west)
}

//Returns all squares a bishop attacks
pub fn bishop_attacks(sq:Square,occ:Bitboard)->Bitboard{
    ray_attacks(sq, occ, Bitboard::north_east)
        | ray_attacks(sq, occ, Bitboard::north_west)
        | ray_attacks(sq, occ, Bitboard::south_east)
        | ray_attacks(sq, occ, Bitboard::south_west)
}

//Returns all squares a queen attacks
pub fn queen_attacks(sq:Square,occ:Bitboard)->Bitboard{
    rook_attacks(sq, occ) | bishop_attacks(sq, occ)
}

//ATTACKDETECTION
// Returns whether a square is attacked by the specified color
pub fn is_attacked(board:&Board,sq:Square,by:Color)->bool{
    let occ = board.all_pieces;

    if !(pawn_attacks(by.opposite(), sq) & board.piece_bitboard(by, PieceType::Pawn)).is_empty(){
        return true;
    }
    if !(knight_attacks(sq) & board.piece_bitboard(by, PieceType::Knight)).is_empty(){
        return true;
    }
    if !(king_attacks(sq) & board.piece_bitboard(by, PieceType::King)).is_empty(){
        return true;
    }
    let diagonal = board.piece_bitboard(by, PieceType::Bishop) | board.piece_bitboard(by, PieceType::Queen);
    if !(bishop_attacks(sq, occ) & diagonal).is_empty(){
        return true;
    }
    let straight = board.piece_bitboard(by, PieceType::Rook) | board.piece_bitboard(by, PieceType::Queen);
    !(rook_attacks(sq, occ) & straight).is_empty()
}

//Returns whether the side to move is in check
pub fn in_check(board:&Board)->bool{
    let us=board.side_to_move;
    match board.king_square(us){
        Some(k) => is_attacked(board, k, us.opposite()),
        None => false,
    }
}

//PSEUDOLEGALGENERATION
//List of pseudo-legal moves
pub fn pseudo_legal_moves(board:&Board)->MoveList{
    generate(board, false)
}

//Captures, en passant and promotions only (for quiescence)
pub fn pseudo_legal_captures(board:&Board)->MoveList{
    generate(board, true)
}

fn generate(board:&Board,tactical_only:bool)->MoveList{
    let mut list = MoveList::new();
    let us = board.side_to_move;
    gen_pawns(board, us, &mut list, tactical_only);
    gen_piece_moves(board, us, &mut list, tactical_only);
    gen_king_moves(board, us, &mut list, tactical_only);
    if !tactical_only{
        gen_castling(board, us, &mut list);
    }
    list
}

//Returns a square offset by the given amount
fn offset(sq:Square,delta:i8)->Square{
    Square((sq.0 as i8 + delta) as u8)
}

//Adds moves to target squares, marking captures of enemy pieces
fn push_targets(list:&mut MoveList,from:Square,mut targets:Bitboard,enemy: Bitboard){
    while let Some(to) = targets.pop_least_significant_bit(){
        let kind = if enemy.contains(to) { MoveKind::Capture } else { MoveKind::Quiet };
        list.push(Move::new(from, to, kind));
    }
}

//Adds all four possible promotion moves, including captures when applicable
fn push_promotions(list:&mut MoveList,from:Square,to:Square,capture:bool){
    for piecetype in [PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight]{
        let kind = if capture{
            MoveKind::PromotionCapture(piecetype)
        }else{
            MoveKind::Promotion(piecetype)
        };
        list.push(Move::new(from, to, kind));
    }
}

//Generates a list of all legal pawn moves
fn gen_pawns(board:&Board,us:Color,list:&mut MoveList,tactical_only:bool){
    let (forward, start_rank, promo_rank): (i8, u8, u8) = match us{
        Color::White => (8, 1, 7),
        Color::Black => (-8, 6, 0),
    };
    let occ = board.all_pieces;
    let enemy = board.all_pieces_by_color[us.opposite() as usize];
    let mut pawns = board.piece_bitboard(us, PieceType::Pawn);

    while let Some(from)=pawns.pop_least_significant_bit(){
        //Single and double pawn pushes
        let one = offset(from, forward);
        if !occ.contains(one){
            if one.rank() == promo_rank{
                push_promotions(list, from, one, false);
            }else if !tactical_only{
                list.push(Move::new(from, one, MoveKind::Quiet));
                if from.rank() == start_rank{
                    let two = offset(from, 2 * forward);
                    if !occ.contains(two){
                        list.push(Move::new(from, two, MoveKind::DoublePawnPush));
                    }
                }
            }
        }

        //Captures
        let mut targets = pawn_attacks(us, from) & enemy;
        while let Some(to) = targets.pop_least_significant_bit(){
            if to.rank() == promo_rank{
                push_promotions(list, from, to, true);
            }else{
                list.push(Move::new(from, to, MoveKind::Capture));
            }
        }

        //En passant
        if let Some(ep) = board.en_passant{
            if pawn_attacks(us, from).contains(ep){
                list.push(Move::new(from, ep, MoveKind::EnPassant));
            }
        }
    }
}

//Returns the attack squares for a sliding piece
fn slider_attacks(piece:PieceType,sq:Square,occ:Bitboard)->Bitboard{
    match piece{
        PieceType::Bishop => bishop_attacks(sq, occ),
        PieceType::Rook => rook_attacks(sq, occ),
        PieceType::Queen => queen_attacks(sq, occ),
        _ => Bitboard::EMPTY,
    }
}

//Generates a MoveList for knights, bishops, rooks, and queens
fn gen_piece_moves(board:&Board,us:Color,list:&mut MoveList,tactical_only:bool){
    let own = board.all_pieces_by_color[us as usize];
    let enemy = board.all_pieces_by_color[us.opposite() as usize];
    let occ = board.all_pieces;

    for piecetype in [PieceType::Knight, PieceType::Bishop, PieceType::Rook, PieceType::Queen]{
        let mut pieces = board.piece_bitboard(us, piecetype);
        while let Some(from) = pieces.pop_least_significant_bit(){
            let attacks = match piecetype{
                PieceType::Knight => knight_attacks(from),
                _ => slider_attacks(piecetype, from, occ),
            };
            let mut targets = attacks & !own;
            if tactical_only { targets = targets & enemy; }
            push_targets(list, from, targets, enemy);
        }
    }
}

//Generates MoveList for the king
fn gen_king_moves(board:&Board,us:Color,list:&mut MoveList,tactical_only:bool){
    let own = board.all_pieces_by_color[us as usize];
    let enemy = board.all_pieces_by_color[us.opposite() as usize];
    if let Some(from) = board.king_square(us){
        let mut targets = king_attacks(from) & !own;
        if tactical_only { targets = targets & enemy; }
        push_targets(list, from, targets, enemy);
    }
}

//Generates MoveList for castling
fn gen_castling(board:&Board,us:Color,list:&mut MoveList){
    let them = us.opposite();
    let rank: u8 = match us{
        Color::White => 0,
        Color::Black => 7,
    };
    let king_sq = Square::new(4, rank);

    if !board.piece_bitboard(us, PieceType::King).contains(king_sq){
        return;
    }
    if is_attacked(board, king_sq, them){
        return;
    }

    let (ks_right, qs_right) = match us{
        Color::White => (CastlingRights::WHITE_KINGSIDE, CastlingRights::WHITE_QUEENSIDE),
        Color::Black => (CastlingRights::BLACK_KINGSIDE, CastlingRights::BLACK_QUEENSIDE),
    };
    let rooks = board.piece_bitboard(us, PieceType::Rook);
    let occ = board.all_pieces;

    if board.castling.has(ks_right) && rooks.contains(Square::new(7, rank)){
        let f = Square::new(5, rank);
        let g = Square::new(6, rank);
        if !occ.contains(f) && !occ.contains(g) && !is_attacked(board, f, them) && !is_attacked(board, g, them){
            list.push(Move::new(king_sq, g, MoveKind::KingsideCastle));
        }
    }

    if board.castling.has(qs_right) && rooks.contains(Square::new(0, rank)){
        let b = Square::new(1, rank);
        let c = Square::new(2, rank);
        let d = Square::new(3, rank);
        if !occ.contains(b) && !occ.contains(c) && !occ.contains(d) && !is_attacked(board, c, them) && !is_attacked(board, d, them){
            list.push(Move::new(king_sq, c, MoveKind::QueensideCastle));
        }
    }
}

//LEGALMOVES
//Generates legal MoveList
pub fn legal_moves(board:&mut Board)->MoveList{
    let us = board.side_to_move;
    let pseudo = pseudo_legal_moves(board);
    let mut legal = MoveList::new();

    for m in pseudo.iter(){
        let undo = board.make_move(m);
        let king_safe = match board.king_square(us){
            Some(k) => !is_attacked(board, k, us.opposite()),
            None => true,
        };
        board.unmake_move(m, undo);

        if king_safe{
            legal.push(m);
        }
    }
    legal
}


//PERFT
pub fn perft(board:&mut Board,depth:u32)->u64{
    if depth == 0{return 1;}
    let moves = legal_moves(board);
    if depth == 1{
        return moves.len() as u64;
    }

    let mut nodes = 0;
    for m in moves.iter(){
        let undo = board.make_move(m);
        nodes += perft(board, depth - 1);
        board.unmake_move(m, undo);
    }
    nodes
}

//Counts and prints the nodes searched for each legal move at the root
pub fn perft_divide(board:&mut Board,depth:u32)->u64{
    let moves = legal_moves(board);
    let mut total = 0;

    for m in moves.iter(){
        let undo = board.make_move(m);
        let n = perft(board, depth.saturating_sub(1));
        board.unmake_move(m, undo);
        println!("{}: {}", m.to_uci(), n);
        total += n;
    }
    println!("Nodes searched: {}", total);
    total
}