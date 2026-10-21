mod board;
mod evaluation;
mod fen;
mod movegen;
mod search;
mod uci;
mod zobrist;

fn main() {
    uci::run();
}

// main.rs — Entry point and UCI protocol
//
// Declare the modules: board, movegen, search, eval.
//
// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------
// - main() builds the shared state once (attack tables, the current Board)
//   and starts the UCI loop.
// - Optional: if the program is started with command-line arguments such as
//   `perft <depth> [fen]`, run perft directly and exit (handy for testing
//   without a GUI).
//
// ---------------------------------------------------------------------------
// UCI loop
// ---------------------------------------------------------------------------
// - Read lines from stdin until "quit" or EOF. Split each line into tokens
//   and dispatch on the first token.
// - Always flush stdout after writing a response.
// - Ignore unknown commands instead of crashing.
//
// Commands to support:
//   uci         -> print "id name ...", "id author ...", then "uciok"
//   isready     -> print "readyok"
//   ucinewgame  -> reset the board and clear any search state (e.g. hash table)
//   position    -> "position startpos [moves e2e4 e7e5 ...]"
//                  "position fen <fen> [moves ...]"
//                  Set up the board from startpos or the FEN, then apply each
//                  listed move in order. Each move string must be matched
//                  against the legal moves of the current position (this is
//                  how promotion letters and castling are resolved).
//   go          -> parse the parameters, then start a search:
//                    depth <n>
//                    movetime <ms>
//                    wtime / btime / winc / binc / movestogo
//                    infinite
//                    perft <n>    (non-standard but very useful)
//                  Work out a time budget from the clock parameters
//                  (for example: remaining time / 30 + increment / 2).
//                  When the search ends, print "bestmove <move>".
//   stop        -> make the running search stop and report its best move
//   d / print   -> (debug) print the board and the FEN
//   quit        -> exit
//
// Search thread:
// - Run the search on a separate thread so that "stop" and "isready" can
//   still be read while searching. Share a stop flag (an atomic bool)
//   between the UCI thread and the search thread.
//
// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------
// - "info depth <d> score cp <x> nodes <n> nps <n> time <ms> pv <moves>"
//   is sent by the search after each completed iteration. Use
//   "score mate <n>" when a forced mate is found.
// - Move text is long algebraic notation: "e2e4", "e7e8q" (promotion letter
//   in lowercase).
//
// ---------------------------------------------------------------------------
// Perft command
// ---------------------------------------------------------------------------
// - "go perft <n>" prints each root move with its node count ("divide"),
//   then the total. This makes it easy to compare against reference engines
//   and find which move generation branch is wrong.