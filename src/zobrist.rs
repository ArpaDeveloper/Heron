
//ZOBRIST HASH
pub struct Zobrist{
    pub pieces:[[[u64;64];6];2], //768 Keys, pieces[color][piece type][square] 
    pub side_to_move:u64, //1 key, which is XOR when Black to move else nothing
    pub castling:[u64;16], //16 keys, one for each value of 4 castling bits (0..15)
    pub en_passant_file:[u64;8], //8 keys, one per file, rank is implied by whose turn it is
}

//Pseudo random key generator (SplitMix64)
const fn next_random(state:u64)->(u64,u64){
    let state=state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (state, z ^ (z >> 31))
}

impl Zobrist{
    //Creates new Zobrist Hash for position
    pub const fn new()->Zobrist{
        let mut state:u64 = 0x1200_2202_2004_AAAA;

        let mut pieces = [[[0u64; 64]; 6]; 2];
        let mut c = 0;
        while c < 2{
            let mut p = 0;
            while p < 6{
                let mut s = 0;
                while s < 64{
                    let(st,v) = next_random(state);
                    state = st;
                    pieces[c][p][s] = v;
                    s += 1;
                }
                p += 1;
            }
            c += 1;
        }

        let (st, side_to_move) = next_random(state);
        state = st;
        
        let mut castling = [0u64;16];
        let mut i = 0;
        while i < 16 {
            let (st, v) = next_random(state);
            state = st;
            castling[i] = v;
            i += 1;
        }

        let mut en_passant_file = [0u64;8];
        let mut f = 0;
        while f < 8 {
            let (st, v) = next_random(state);
            state = st;
            en_passant_file[f] = v;
            f += 1;
        }

        Zobrist{pieces,side_to_move,castling,en_passant_file}
    }
}



//Built at compile time, so there is no setup code at startup
pub static ZOBRIST: Zobrist=Zobrist::new();
