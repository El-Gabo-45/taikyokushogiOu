//! Head-to-head strength testing: play N games between two search configs
//! and report the W/L/D score. This is the minimal tool for measuring Elo
//! gains (handcrafted vs NNUE, depth A vs depth B, config vs config).
//!
//! Games are deterministic (seeded LCG opening randomization), so a given
//! invocation is reproducible; vary the game count / opening depth to widen
//! the sample. For publishable claims, run many games and apply SPRT or a
//! confidence interval on the W/L/D counts.
//!
//! Usage:
//!   cargo run --release --example match_race -- [games] [depth_a] [depth_b] [time_ms] [opening_plies]
//! Defaults: 4 games, A=depth 3, B=depth 2, 5000ms budget, 4 random opening plies.

use taikyokushogi::{Board, GameResult};

struct Config {
    name: &'static str,
    depth: u32,
    time_ms: u64,
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn pick(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() as usize) % n }
    }
}

const MAX_GAME_PLIES: u32 = 300; // adjudicate longer games as draws

fn play_game(game: usize, cfg_black: &Config, cfg_white: &Config, opening_plies: usize) -> i8 {
    // 1 = black (cfg_black) wins, -1 = white wins, 0 = draw
    let mut rng = Lcg(0x9E37_79B9_7F4A_7C15 ^ (game as u64).wrapping_mul(0x85EB_CA6B));
    let mut board = Board::initial();

    // Deterministic random opening so games don't repeat identically.
    for _ in 0..opening_plies {
        let moves = board.legal_moves();
        if moves.is_empty() { break; }
        board.apply(&moves[rng.pick(moves.len())]);
    }

    for _ply in 0..MAX_GAME_PLIES {
        if let Some(result) = board.game_result() {
            return match result {
                GameResult::BlackWins => 1,
                GameResult::WhiteWins => -1,
                GameResult::Draw => 0,
            };
        }
        let cfg = if board.side_to_move() == taikyokushogi::Color::Black { cfg_black } else { cfg_white };
        let r = board.search(cfg.depth, cfg.time_ms);
        match r.best_move {
            Some(m) => board.apply(&m),
            None => {
                // No move found: side to move loses (royal captured or no moves).
                return if board.side_to_move() == taikyokushogi::Color::Black { -1 } else { 1 };
            }
        }
    }
    0 // adjudicated draw at the ply cap
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let games: usize = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(4);
    let cfg_a = Config {
        name: "A",
        depth: args.get(2).map(|s| s.parse().unwrap()).unwrap_or(3),
        time_ms: args.get(4).map(|s| s.parse().unwrap()).unwrap_or(5000),
    };
    let cfg_b = Config {
        name: "B",
        depth: args.get(3).map(|s| s.parse().unwrap()).unwrap_or(2),
        time_ms: args.get(4).map(|s| s.parse().unwrap()).unwrap_or(5000),
    };
    let opening_plies: usize = args.get(5).map(|s| s.parse().unwrap()).unwrap_or(4);

    println!(
        "match_race: {} games | A: d{} {}ms | B: d{} {}ms | {} opening plies",
        games, cfg_a.depth, cfg_a.time_ms, cfg_b.depth, cfg_b.time_ms, opening_plies
    );

    let (mut a_wins, mut b_wins, mut draws) = (0u32, 0u32, 0u32);
    for game in 0..games {
        // Alternate colors: even games A takes black.
        let score = if game % 2 == 0 {
            play_game(game, &cfg_a, &cfg_b, opening_plies)
        } else {
            -play_game(game, &cfg_b, &cfg_a, opening_plies)
        };
        match score {
            1 => { a_wins += 1; println!("game {:>3}: {} wins", game, cfg_a.name); }
            -1 => { b_wins += 1; println!("game {:>3}: {} wins", game, cfg_b.name); }
            _ => { draws += 1; println!("game {:>3}: draw", game); }
        }
    }

    let decided = a_wins + b_wins;
    let a_pct = if decided > 0 { 100.0 * a_wins as f64 / decided as f64 } else { 50.0 };
    println!("\n=== RESULT ===");
    println!("{} (d{}): {} wins", cfg_a.name, cfg_a.depth, a_wins);
    println!("{} (d{}): {} wins", cfg_b.name, cfg_b.depth, b_wins);
    println!("draws: {} (of {} games)", draws, games);
    println!("{} score: {:.1}% of decided games", cfg_a.name, a_pct);
    println!("\nFor a publishable claim: run >= 1000 games and compute a");
    println!("confidence interval or SPRT bounds on these W/L/D counts.");
}
