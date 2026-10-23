/*
    Engine: Heron
    Author: ArpaDeveloper

    File: fen.rs
    This file contains FEN parsing and handling
*/

use crate::board::{Board, CastlingRights, Color, PieceType, Square};

//Starting position as FEN
pub const START_FEN:&str="rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

///Turn FEN char (r | R) to (Color, PieceType)
pub fn piece_from_fen_char(c: char)->Option<(Color,PieceType)>{
    let piecetype = PieceType::from_char(c)?;
    let color = if c.is_ascii_uppercase(){
        Color::White
    }else{
        Color::Black
    };
    Some((color,piecetype))
}

//Handle FEN parsing for CastlingRights (KQkq)
impl CastlingRights {
    //Turn FEN to CastlingRights
    pub fn from_fen(s: &str)->Option<CastlingRights>{
        if s == "-"{
            return Some(CastlingRights::NONE);
        }
        if s.is_empty(){
            return None;
        }

        let mut rights = CastlingRights::NONE;
        for c in s.chars(){
            let flag = match c{
                'K' => CastlingRights::WHITE_KINGSIDE,
                'Q' => CastlingRights::WHITE_QUEENSIDE,
                'k' => CastlingRights::BLACK_KINGSIDE,
                'q' => CastlingRights::BLACK_QUEENSIDE,
                _ => return None, 
            };
            if rights.has(flag){
                return None; 
            }
            rights.add(flag);
        }
        Some(rights)
    }

    //Turn CastlingRights to FEN
    pub fn to_fen(self)->String{
        if self.0 == 0{
            return "-".to_string();
        }
        let mut s = String::new();
        if self.has(Self::WHITE_KINGSIDE)
            {s.push('K');}
        if self.has(Self::WHITE_QUEENSIDE)
            {s.push('Q');}
        if self.has(Self::BLACK_KINGSIDE)
            {s.push('k');}
        if self.has(Self::BLACK_QUEENSIDE)
            {s.push('q');}
        s
    }
}

//Handle FEN parsing for Board (rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1)
impl Board {

    //Builds the start position Board
    pub fn startpos()->Board{
        Board::from_fen(START_FEN).unwrap()
    }

    //Turns FEN to Board
    pub fn from_fen(fen:&str)->Result<Board,String>{
        let mut parts = fen.split_whitespace();
        let mut board = Board::empty();

        //Piece placement
        let placement = parts.next().ok_or("FEN: missing piece placement")?;
        let ranks: Vec<&str> = placement.split('/').collect();
        if ranks.len() != 8{
            return Err(format!("FEN: expected 8 ranks, found {}", ranks.len()));
        }

        for (i, rank_str) in ranks.iter().enumerate(){
            let rank = 7-i as u8;
            let mut file:u8 = 0;

            for c in rank_str.chars() {
                if let Some(d) = c.to_digit(10){
                    if !(1..=8).contains(&d){
                        return Err(format!("FEN: bad empty-square count '{}'", c));
                    }
                    file += d as u8;
                    if file > 8{
                        return Err("FEN: rank has more than 8 squares".to_string());
                    }
                } else {
                    let (color, piecetype) = piece_from_fen_char(c).ok_or_else(|| format!("FEN: bad piece '{}'", c))?;
                    if file >= 8{
                        return Err("FEN: rank has more than 8 squares".to_string());
                    }
                    board.put_piece(color, piecetype, Square::new(file,rank));
                    file += 1;
                }
            }

            if file != 8 {
                return Err(format!("FEN: rank {} does not have 8 squares", rank + 1));
            }
        }

        //Side to move
        board.side_to_move = match parts.next().ok_or("FEN: missing side to move")?{
            "w" => Color::White,
            "b" => Color::Black,
            s => return Err(format!("FEN: bad side to move '{}'", s)),
        };

        //Castling rights
        let castling = parts.next().ok_or("FEN: missing castling rights")?;
        board.castling = CastlingRights::from_fen(castling).ok_or_else(|| format!("FEN: bad castling field '{}'", castling))?;

        //En passant square
        let ep = parts.next().ok_or("FEN: missing en passant field")?;
        board.en_passant = if ep == "-"{
            None
        }else{
            let sq = Square::from_algebraic(ep).ok_or_else(|| format!("FEN: bad en passant square '{}'", ep))?;
            let expected_rank = if board.side_to_move == Color::White{ 5 } else { 2 };
            if sq.rank() != expected_rank{
                return Err(format!("FEN: en passant square '{}' is on the wrong rank", ep));
            }
            Some(sq)
        };

        //Clocks 
        board.halfmove_clock = match parts.next(){
            Some(s) => s.parse().map_err(|_| format!("FEN: bad halfmove clock '{}'", s))?,
            None => 0,
        };
        board.fullmove_number = match parts.next(){
            Some(s) => s.parse().map_err(|_| format!("FEN: bad fullmove number '{}'", s))?,
            None => 1,
        };

        //One King per side
        for color in [Color::White,Color::Black] {
            if board.piece_bitboard(color,PieceType::King).count()!=1{
                return Err(format!("FEN: {:?} must have exactly one king", color));
            }
        }
        board.hash = board.compute_hash();
        Ok(board)
    }

    //Turns Board to FEN
    pub fn to_fen(&self)->String{
        let mut s = String::new();

        //Pieces
        for rank in (0..8).rev(){
            let mut empty = 0;
            for file in 0..8 {
                match self.piece_at(Square::new(file,rank)){
                    Some((color, piecetype))=>{
                        if empty > 0{
                            s.push_str(&empty.to_string());
                            empty = 0;
                        }
                        let c = piecetype.to_char();
                        s.push(if color == Color::White { c.to_ascii_uppercase() } else { c });
                    }
                    None => empty += 1,
                }
            }
            if empty > 0{
                s.push_str(&empty.to_string());
            }
            if rank > 0{
                s.push('/');
            }
        }

        //Side to Move
        s.push(' ');
        s.push(if self.side_to_move == Color::White { 'w' } else { 'b' });

        //CastlingRights
        s.push(' ');
        s.push_str(&self.castling.to_fen());

        //En passant
        s.push(' ');
        match self.en_passant{
            Some(sq) => s.push_str(&sq.to_algebraic()),
            None => s.push('-'),
        }
        //Clocks
        s.push_str(&format!(" {} {}", self.halfmove_clock, self.fullmove_number));
        s
    }
}
