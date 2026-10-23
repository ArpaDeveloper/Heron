/*
    Engine: Heron
    Author: ArpaDeveloper

    File: transpositiontable.rs
    This file contains struct and methods for Transposition tables
*/

use crate::movegen::Move;

//TRANSPOSITIONTABLES
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Bound{
    Exact, 
    Lower, 
    Upper, 
}

#[derive(Copy, Clone)]
pub struct TtEntry{
    pub key: u64,
    pub score: i32,
    pub best: Option<Move>,
    pub depth: u8,
    pub bound: Bound,
}

const EMPTY: TtEntry = TtEntry { key: 0, score: 0, best: None, depth: 0, bound: Bound::Exact };

pub struct TranspositionTable{
    entries: Vec<TtEntry>,
    mask: usize, 
}

impl TranspositionTable{
    //Create new transposition table
    pub fn new(megabytes:usize)->TranspositionTable{
        let bytes = megabytes.max(1) * 1024 * 1024;
        let wanted = (bytes / std::mem::size_of::<TtEntry>()).max(1);
        let size = 1usize << (usize::BITS - 1 - wanted.leading_zeros()); 
        TranspositionTable { entries: vec![EMPTY; size], mask: size - 1 }
    }

    //Clear transposition table
    pub fn clear(&mut self){
        self.entries.fill(EMPTY);
    }

    //Returns the entry only if it belongs to this exact position
    pub fn probe(&self,key:u64)->Option<TtEntry>{
        let e = self.entries[key as usize & self.mask];
        if e.key == key { Some(e) } else { None }
    }

    //Stores a transposition-table entry
    pub fn store(&mut self,key:u64,depth:u32,score:i32,bound:Bound,best:Option<Move>){
        let depth = depth.min(255) as u8;
        let slot = &mut self.entries[key as usize & self.mask];
        let mut best = best;

        if slot.key == key{
            if best.is_none() { best = slot.best; } 
            if bound != Bound::Exact && depth < slot.depth { return; }
        }
        *slot = TtEntry { key, score, best, depth, bound };
    }
}