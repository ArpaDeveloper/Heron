/*
    Engine: Heron
    Author: ArpaDeveloper

    File: Board.rs
    This file handles board representation and board management
*/

use crate::zobrist::ZOBRIST;
use crate::movegen::{Move, MoveKind};
use std::ops::{BitAnd, BitOr, BitOrAssign, Not};

//BOARD
#[derive(Clone, Debug)]
pub struct Board{
    pub pieces:[[Bitboard; 6];2], //pieces[color][piece type]
    pub all_pieces_by_color:[Bitboard; 2],
    pub all_pieces:Bitboard,
    pub side_to_move:Color,
    pub castling:CastlingRights,
    pub en_passant:Option<Square>, //Square the pawn would be after en passant
    pub halfmove_clock:u32, //For 50-move rule
    pub fullmove_number:u32,
    pub hash:u64, //Zobrist hash of the position
    pub history: Vec<u64>, //hashes of earlier positions in the game
    pub mailbox: [Option<(Color, PieceType)>; 64], //what stands on each square
}

//UNDO: state that make_move changes and that can't be rebuilt from the move alone
#[derive(Copy, Clone, Debug)]
pub struct Undo{
    pub captured: Option<PieceType>,
    pub castling: CastlingRights,
    pub en_passant: Option<Square>,
    pub halfmove_clock: u32,
    pub fullmove_number: u32,
    pub hash: u64,
}

#[derive(Copy, Clone, Debug)]
pub struct NullUndo{
    en_passant: Option<Square>,
    halfmove_clock: u32,
    hash: u64,
}

impl Board{
    //Creates empty board
    pub fn empty()->Board{
        Board{
            pieces:[[Bitboard::EMPTY;6];2],
            all_pieces_by_color:[Bitboard::EMPTY; 2],
            all_pieces: Bitboard::EMPTY,
            side_to_move:Color::White,
            castling: CastlingRights::NONE,
            en_passant:None,
            halfmove_clock:0,
            fullmove_number:1,
            hash:0,
            history:Vec::new(),
            mailbox:[None; 64],
        }
    }

    //Getter for a bitboard of specific (Color, PieceType)
    pub fn piece_bitboard(&self, color:Color, piecetype:PieceType)->Bitboard{
        self.pieces[color as usize][piecetype as usize]
    }

    //Place piece on a Square
    pub fn put_piece(&mut self, color:Color, piecetype:PieceType, sq:Square){
        self.pieces[color as usize][piecetype as usize].set(sq);
        self.all_pieces_by_color[color as usize].set(sq);
        self.all_pieces.set(sq);
        self.hash ^= ZOBRIST.pieces[color as usize][piecetype as usize][sq.0 as usize];
        self.mailbox[sq.0 as usize] = Some((color, piecetype));
    }

    //Remove piece from a Square
    pub fn remove_piece(&mut self, color:Color, piecetype:PieceType, sq:Square){
        self.pieces[color as usize][piecetype as usize].clear(sq);
        self.all_pieces_by_color[color as usize].clear(sq);
        self.all_pieces.clear(sq);
        self.hash ^= ZOBRIST.pieces[color as usize][piecetype as usize][sq.0 as usize];
        self.mailbox[sq.0 as usize] = None;
    }

    //Check whats on specific Square
    pub fn piece_at(&self, sq:Square)->Option<(Color,PieceType)>{
        self.mailbox[sq.0 as usize]
    }

    //Computes the Zobrist hash from scratch
    pub fn compute_hash(&self)->u64{
        let mut h:u64 = 0;
        for color in [Color::White,Color::Black]{
            for piecetype in PieceType::ALL{
                let mut bitboard = self.piece_bitboard(color,piecetype);
                while let Some(sq) = bitboard.pop_least_significant_bit(){
                    h ^= ZOBRIST.pieces[color as usize][piecetype as usize][sq.0 as usize];
                }
            }
        }
        if self.side_to_move == Color::Black{
            h ^= ZOBRIST.side_to_move;
        }
        h ^= ZOBRIST.castling[self.castling.0 as usize];
        if let Some(sq) = self.en_passant{
            h ^= ZOBRIST.en_passant_file[sq.file() as usize];
        }
        h
    }

    //Getter for King's square for a specific color
    pub fn king_square(&self, color:Color)->Option<Square>{
        self.piece_bitboard(color, PieceType::King).least_significant_bit()
    }

    //Applies a pseudo-legal move and saves the state needed to undo it
    pub fn make_move(&mut self, m: Move)->Undo{
        let us = self.side_to_move;
        let them = us.opposite();
        let from = m.from();
        let to = m.to();

        //Type of move & piece
        let captured = match m.kind(){
            MoveKind::EnPassant => Some(PieceType::Pawn),
            MoveKind::Capture | MoveKind::PromotionCapture(_) => self.piece_at(to).map(|(_, p)| p),
            _ => None,
        };

        //Saved state
        let undo = Undo{
            captured,
            castling: self.castling,
            en_passant: self.en_passant,
            halfmove_clock: self.halfmove_clock,
            fullmove_number: self.fullmove_number,
            hash: self.hash,
        };

        self.history.push(self.hash);

        //En passant
        if let Some(ep) = self.en_passant{
            self.hash ^= ZOBRIST.en_passant_file[ep.file() as usize];
        }
        self.en_passant = None;

        let moving = self.piece_at(from).expect("make_move: no piece on from square").1;
        self.remove_piece(us, moving, from);

        //Capture
        if let Some(cap) = captured{
            self.remove_piece(them, cap, capture_square(m));
        }

        //Promotion
        let placed = m.promotion_piece().unwrap_or(moving);
        self.put_piece(us, placed, to);

        //Castling
        if let Some((rook_from, rook_to)) = castle_rook_squares(m){
            self.remove_piece(us, PieceType::Rook, rook_from);
            self.put_piece(us, PieceType::Rook, rook_to);
        }

        let old = self.castling.0;
        let new = old & !(lost_castling_rights(from) | lost_castling_rights(to));
        self.hash ^= ZOBRIST.castling[old as usize] ^ ZOBRIST.castling[new as usize];
        self.castling = CastlingRights(new);

        //Double pawn push
        if m.kind() == MoveKind::DoublePawnPush{
            let ep = Square::new(from.file(), (from.rank() + to.rank()) / 2);
            self.en_passant = Some(ep);
            self.hash ^= ZOBRIST.en_passant_file[ep.file() as usize];
        }

        //Clocks
        self.halfmove_clock = if moving == PieceType::Pawn || captured.is_some(){
            0
        }else{
            self.halfmove_clock + 1
        };
        if us == Color::Black{
            self.fullmove_number += 1;
        }

        self.side_to_move = them;
        self.hash ^= ZOBRIST.side_to_move;
        undo
    }

    //Reverses a move and restores the previous board state
    pub fn unmake_move(&mut self, m: Move, undo: Undo){
        let us = self.side_to_move.opposite(); 
        let them = self.side_to_move;
        let from = m.from();
        let to = m.to();

        let placed = self.piece_at(to).expect("unmake_move: no piece on to square").1;
        self.remove_piece(us, placed, to);
        let moving = if m.promotion_piece().is_some(){ PieceType::Pawn }else{ placed };
        self.put_piece(us, moving, from);

        if let Some(cap) = undo.captured{
            self.put_piece(them, cap, capture_square(m));
        }

        if let Some((rook_from, rook_to)) = castle_rook_squares(m){
            self.remove_piece(us, PieceType::Rook, rook_to);
            self.put_piece(us, PieceType::Rook, rook_from);
        }

        self.side_to_move = us;
        self.castling = undo.castling;
        self.en_passant = undo.en_passant;
        self.halfmove_clock = undo.halfmove_clock;
        self.fullmove_number = undo.fullmove_number;
        self.hash = undo.hash;
        self.history.pop();
    }

    //Makes a null move
    pub fn make_null_move(&mut self)->NullUndo{
        self.history.push(self.hash);
        let undo = NullUndo { en_passant: self.en_passant, halfmove_clock: self.halfmove_clock, hash: self.hash };
        if let Some(ep) = self.en_passant{
            self.hash ^= ZOBRIST.en_passant_file[ep.file() as usize];
        }
        self.en_passant = None;
        self.halfmove_clock += 1;
        self.side_to_move = self.side_to_move.opposite();
        self.hash ^= ZOBRIST.side_to_move;
        undo
    }

    //Unmakes a null move
    pub fn unmake_null_move(&mut self,undo:NullUndo){
        self.side_to_move = self.side_to_move.opposite();
        self.en_passant = undo.en_passant;
        self.halfmove_clock = undo.halfmove_clock;
        self.hash = undo.hash;
        self.history.pop();
    }

    //Checks for repetition
    pub fn is_repetition(&self,ply:i32)->bool{
        let len = self.history.len();
        let max_back = (self.halfmove_clock as usize).min(len);
        let mut count = 0;
        let mut back = 2;
        while back <= max_back{
            if self.history[len - back] == self.hash{
                if back as i32 <= ply { return true; }
                count += 1;
                if count >= 2 { return true; }
            }
            back += 2;
        }
        false
    }
}

//BITBOARD
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub struct Bitboard(pub u64);

impl Bitboard{
    #[allow(dead_code)]
    pub const EMPTY: Bitboard=Bitboard(0);
    #[allow(dead_code)]
    pub const FULL: Bitboard=Bitboard(!0);

    //File & Rank constants for pawn double pushes and promotions
    pub const FILE_A: Bitboard = Bitboard(0x0101_0101_0101_0101);
    pub const FILE_H: Bitboard = Bitboard(0x8080_8080_8080_8080);
    pub const RANK_1: Bitboard = Bitboard(0x0000_0000_0000_00FF);
    pub const RANK_2: Bitboard = Bitboard(0x0000_0000_0000_FF00);
    pub const RANK_7: Bitboard = Bitboard(0x00FF_0000_0000_0000);
    pub const RANK_8: Bitboard = Bitboard(0xFF00_0000_0000_0000);

    //Bitboard shift functions for moving squares in each direction
    pub const fn north(self) -> Bitboard { Bitboard(self.0 << 8) }
    pub const fn south(self) -> Bitboard { Bitboard(self.0 >> 8) }
    pub const fn east(self) -> Bitboard { Bitboard((self.0 << 1) & !Self::FILE_A.0) }
    pub const fn west(self) -> Bitboard { Bitboard((self.0 >> 1) & !Self::FILE_H.0) }
    pub const fn north_east(self) -> Bitboard { self.north().east() }
    pub const fn north_west(self) -> Bitboard { self.north().west() }
    pub const fn south_east(self) -> Bitboard { self.south().east() }
    pub const fn south_west(self) -> Bitboard { self.south().west() }

    //Returns a Bitboard with 1 on the specified square
    pub const fn from_square(sq:Square)->Bitboard{
        Bitboard(1u64 <<sq.0)
    }

    //Checks if the board is empty
    pub const fn is_empty(self)->bool{
        self.0==0
    }

    //Counts the occupied squares
    pub const fn count(self)->u32{
        self.0.count_ones()
    }

    //Checks if square is occupied
    pub const fn contains(self, sq:Square)->bool{
        self.0 & (1u64<<sq.0)!=0
    }

    //Set specific bit (square) on
    pub fn set(&mut self, sq:Square){
        self.0 |= 1u64<<sq.0;
    }

    //Clear specific square
    pub fn clear(&mut self, sq:Square){
        self.0 &= !(1u64<<sq.0);
    }

    //Returns the lowest set bit square
    pub fn least_significant_bit(self)->Option<Square>{
        if self.0 == 0{
            None
        }else{
            Some(Square(self.0.trailing_zeros() as u8))
        }
    }

    //Returns the lowest set bit square and removes it
    pub fn pop_least_significant_bit(&mut self)->Option<Square>{
        let sq=self.least_significant_bit()?;
        self.0 &= self.0-1;
        Some(sq)
    }

     //Returns a bitboard containing every square on the given file
    pub const fn file_mask(file:u8)->Bitboard{
        Bitboard(Self::FILE_A.0 << file)
    }

    //Returns a bitboard of the files immediately adjacent to the given file
    pub const fn adjacent_files(file:u8)->Bitboard{
        let f = Self::file_mask(file);
        Bitboard(f.west().0 | f.east().0) 
    }

    //Returns a bitboard of all ranks strictly ahead of the given rank from the color's perspective
    pub const fn front_ranks(color:Color,rank:u8)->Bitboard{
        match color{
            Color::White => if rank >= 7 { Bitboard(0) } else { Bitboard(!0u64 << (8 * (rank + 1))) },
            Color::Black => Bitboard((1u64 << (8 * rank)) - 1),
        }
    }

    //Returns a bitboard containing the rank immediately behind the given rank from the color's perspective
    pub const fn rank_behind(color:Color,rank:u8)->Bitboard{
        match color{
            Color::White => if rank == 0 { Bitboard(0) } else { Bitboard(0xFFu64 << (8 * (rank - 1))) },
            Color::Black => if rank >= 7 { Bitboard(0) } else { Bitboard(0xFFu64 << (8 * (rank + 1))) },
        }
    }
}

//Implements bitwise AND (&) for bitboards
impl BitAnd for Bitboard {
    type Output = Bitboard;
    fn bitand(self, rhs: Bitboard) -> Bitboard {
        Bitboard(self.0 & rhs.0)
    }
}

//Implements bitwise OR (|) for bitboards
impl BitOr for Bitboard {
    type Output = Bitboard;
    fn bitor(self, rhs: Bitboard) -> Bitboard {
        Bitboard(self.0 | rhs.0)
    }
}

//Implements bitwise OR assignment (|=) for bitboards
impl BitOrAssign for Bitboard {
    fn bitor_assign(&mut self, rhs: Bitboard) {
        self.0 |= rhs.0;
    }
}

//Implements bitwise NOT (!) for bitboards
impl Not for Bitboard {
    type Output = Bitboard;
    fn not(self) -> Bitboard {
        Bitboard(!self.0)
    }
}

//SQUARE
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Square(pub u8);

impl Square{
    //Return Square from file & rank (Both go from 0..7)
    pub const fn new(file:u8, rank:u8)->Square{
        Square(rank*8+file)
    }

    //Column: 0=a, 7=h
    pub const fn file(self)->u8{
        self.0 & 7
    }

    //Row: 0=rank1, 7=rank 8
    pub const fn rank(self)->u8{
        self.0>>3
    }

    //Parses algebraic (e4) to Square
    pub fn from_algebraic(s: &str)->Option<Square>{
        let b = s.as_bytes();
        if b.len()!=2{
            return None;
        }
        let file=b[0].checked_sub(b'a')?;
        let rank = b[1].checked_sub(b'1')?;
        if file < 8 && rank < 8{
            Some(Square::new(file, rank))
        }else{
            None
        }
    }

    //Turns Square to algebraic
    pub fn to_algebraic(self)->String{
        format!("{}{}",(b'a' + self.file()) as char,(b'1' + self.rank()) as char)
    }
}

//COLOR
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Color{
    White=0,
    Black=1,
}

impl Color{
    pub const fn opposite(self)->Color{
        match self{
            Color::White=>Color::Black,
            Color::Black=>Color::White,
        }
    }
}

//PIECETYPE
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PieceType{
    Pawn=0,
    Knight=1,
    Bishop=2,
    Rook=3,
    Queen=4,
    King=5,
}

impl PieceType{
    //Way to iterate over all PieceTypes
    pub const ALL: [PieceType; 6] = [
        PieceType::Pawn,
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ];

    //Turn char to PieceType
    pub fn from_char(c: char)->Option<PieceType>{
        match c.to_ascii_lowercase(){
            'p' => Some(PieceType::Pawn),
            'n' => Some(PieceType::Knight),
            'b' => Some(PieceType::Bishop),
            'r' => Some(PieceType::Rook),
            'q' => Some(PieceType::Queen),
            'k' => Some(PieceType::King),
            _ => None,
        }
    }

    //Turn PieceType to char
    pub const fn to_char(self)->char{
        match self{
            PieceType::Pawn => 'p',
            PieceType::Knight => 'n',
            PieceType::Bishop => 'b',
            PieceType::Rook => 'r',
            PieceType::Queen => 'q',
            PieceType::King => 'k',
        }
    }
}

//CASTLINGRIGHTS
#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub struct CastlingRights(pub u8);

impl CastlingRights{
    pub const WHITE_KINGSIDE:u8=0b0001;
    pub const WHITE_QUEENSIDE:u8=0b0010;
    pub const BLACK_KINGSIDE:u8=0b0100;
    pub const BLACK_QUEENSIDE:u8=0b1000;

    pub const NONE:CastlingRights=CastlingRights(0);
    #[allow(dead_code)]
    pub const ALL:CastlingRights=CastlingRights(0b1111);

    //Check if specific CastlingRight is available
    pub const fn has(self, right:u8)->bool{
        self.0 & right!=0
    }

    //Grant CastlingRight
    pub fn add(&mut self, right:u8){
        self.0 |= right;
    }

    //Remove CastlingRight
    #[allow(dead_code)]
    pub fn remove(&mut self, right:u8){
        self.0 &= !right;
    }
}

//HELPERS

//Getter for captured piece's square, accounts for en passant
fn capture_square(m: Move)->Square{
    match m.kind(){
        MoveKind::EnPassant => Square::new(m.to().file(), m.from().rank()),
        _ => m.to(),
    }
}

//Getter for rook's start and end squares for a castling moves
fn castle_rook_squares(m: Move)->Option<(Square, Square)>{
    let rank = m.from().rank();
    match m.kind(){
        MoveKind::KingsideCastle => Some((Square::new(7, rank), Square::new(5, rank))),
        MoveKind::QueensideCastle => Some((Square::new(0, rank), Square::new(3, rank))),
        _ => None,
    }
}

//Getter for CastlingRights that are lost, when rook or king moves
fn lost_castling_rights(sq: Square)->u8{
    match (sq.file(), sq.rank()){
        (4, 0) => CastlingRights::WHITE_KINGSIDE | CastlingRights::WHITE_QUEENSIDE,
        (0, 0) => CastlingRights::WHITE_QUEENSIDE,
        (7, 0) => CastlingRights::WHITE_KINGSIDE,
        (4, 7) => CastlingRights::BLACK_KINGSIDE | CastlingRights::BLACK_QUEENSIDE,
        (0, 7) => CastlingRights::BLACK_QUEENSIDE,
        (7, 7) => CastlingRights::BLACK_KINGSIDE,
        _ => 0,
    }
}