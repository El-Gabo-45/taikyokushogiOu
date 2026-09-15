//! Centralized correctness suite for the engine (runs with `cargo test`).
//!
//! Covers the invariants that used to live only in examples/ tools:
//! TSFEN round trips, perft golden values, apply/undo state restoration,
//! effect-dedup of generated moves, range-capture application, and search
//! returning legal moves without corrupting the board.

use crate::{Board, Color};

// ── Deterministic LCG for reproducible random walks ─────────────
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

// ── TSFEN round trip ────────────────────────────────────────────
#[test]
fn tsfen_roundtrip_initial() {
    let b = Board::initial();
    let tsfen = b.to_tsfen();
    let restored = Board::from_tsfen(&tsfen).expect("initial TSFEN must parse");
    assert_eq!(restored.to_tsfen(), tsfen);
    assert_eq!(restored.piece_count(Color::Black), 402);
    assert_eq!(restored.piece_count(Color::White), 402);
    assert_eq!(restored.legal_moves().len(), 512);
}

#[test]
fn tsfen_roundtrip_after_random_walk() {
    let mut rng = Lcg(0xDEADBEEF);
    let mut board = Board::initial();
    for _ in 0..40 {
        let moves = board.legal_moves();
        if moves.is_empty() { break; }
        board.apply(&moves[rng.pick(moves.len())]);
    }
    let tsfen = board.to_tsfen();
    let restored = Board::from_tsfen(&tsfen).expect("walk TSFEN must parse");
    assert_eq!(restored.to_tsfen(), tsfen);
    assert_eq!(restored.material_score(), board.material_score());
    assert_eq!(restored.legal_moves().len(), board.legal_moves().len());
}

// ── Perft golden values (current tree, dedup ON) ────────────────
#[test]
fn perft_golden_initial() {
    let mut board = Board::initial();
    assert_eq!(perft(&mut board, 1), 512);
    assert_eq!(perft(&mut board, 2), 260_908);
}

fn perft(board: &mut Board, depth: u32) -> u64 {
    if depth == 0 { return 1; }
    let moves = board.legal_moves();
    if depth == 1 { return moves.len() as u64; }
    let mut n = 0;
    for m in &moves {
        board.apply(m);
        n += perft(board, depth - 1);
        board.undo();
    }
    n
}

// ── apply/undo state restoration ────────────────────────────────
#[test]
fn apply_undo_restores_state() {
    let mut rng = Lcg(0xC0FFEE);
    let mut board = Board::initial();
    for ply in 0..80 {
        let before_tsfen = board.to_tsfen();
        let before_mat = board.material_score();
        let before_counts = (board.piece_count(Color::Black), board.piece_count(Color::White));
        let moves = board.legal_moves();
        if moves.is_empty() { break; }
        let m = moves[rng.pick(moves.len())].clone();
        board.apply(&m);
        board.undo();
        assert_eq!(board.to_tsfen(), before_tsfen, "cells diverged at ply {}", ply);
        assert_eq!(board.material_score(), before_mat, "material diverged at ply {}", ply);
        assert_eq!(
            (board.piece_count(Color::Black), board.piece_count(Color::White)),
            before_counts, "piece counts diverged at ply {}", ply
        );
        board.apply(&moves[(ply * 7919 + 13) % moves.len()]);
    }
}

// __APPEND__
#[test]
fn bulk_unwind_restores_state() {
    let mut rng = Lcg(0x5EED);
    let mut board = Board::initial();
    for _ in 0..5 {
        let base = board.to_tsfen();
        let base_mat = board.material_score();
        let mut stack = Vec::new();
        for _ in 0..30 {
            let moves = board.legal_moves();
            if moves.is_empty() { break; }
            let m = moves[rng.pick(moves.len())].clone();
            board.apply(&m);
            stack.push(m);
        }
        while let Some(_m) = stack.pop() {
            board.undo();
        }
        assert_eq!(board.to_tsfen(), base);
        assert_eq!(board.material_score(), base_mat);
    }
}

// ── Effect dedup: no two legal moves may produce the same position ──
#[test]
fn legal_moves_have_no_duplicate_effects() {
    let mut rng = Lcg(0x1234_5678);
    let mut board = Board::initial();
    for _ in 0..15 {
        let moves = board.legal_moves();
        let mut seen = std::collections::HashSet::new();
        for m in &moves {
            let r = m.raw();
            let key = (r.from_sq, r.to_sq, r.promotion, r.mid_sq, r.mid_piece != 0,
                       r.captured_piece, r.range_cap, r.caps_value);
            assert!(seen.insert(key), "duplicate-effect move {:?}->{:?}", r.from_sq, r.to_sq);
        }
        if moves.is_empty() { break; }
        board.apply(&moves[rng.pick(moves.len())]);
    }
}

// ── Range captures must empty every occupied intermediate square ────
#[test]
fn range_capture_empties_intermediates() {
    let mut board = Board::initial();
    let moves = board.legal_moves();
    let rc = moves.iter().find(|m| {
        let r = m.raw();
        r.range_cap && r.caps_value != 0
    }).expect("initial position has range-capture moves").clone();
    let (from, to) = (rc.raw().from_sq as usize, rc.raw().to_sq as usize);
    let (fr, fc) = (from / 36, from % 36);
    let (tr, tc) = (to / 36, to % 36);
    let dr = (tr as i32 - fr as i32).signum();
    let dc = (tc as i32 - fc as i32).signum();
    // Count occupied squares strictly between from and to.
    let mut occupied_between: usize = 0;
    let (mut r, mut c) = (fr as i32 + dr, fc as i32 + dc);
    while (r, c) != (tr as i32, tc as i32) {
        if board.get(r as usize, c as usize).is_some() { occupied_between += 1; }
        r += dr; c += dc;
    }
    let landing_occupied = board.get(tr, tc).is_some() as usize;
    let before_total = board.piece_count(Color::Black) + board.piece_count(Color::White);
    board.apply(&rc);
    let after_total = board.piece_count(Color::Black) + board.piece_count(Color::White);
    assert_eq!(before_total - after_total, occupied_between + landing_occupied,
        "range capture must remove ALL occupied path squares (any color)");
    board.undo();
    assert_eq!(board.piece_count(Color::Black) + board.piece_count(Color::White), before_total);
}

// ── Search: returns a legal move and leaves the board untouched ─────
#[test]
fn search_returns_legal_move_and_preserves_state() {
    let mut board = Board::initial();
    let before = board.to_tsfen();
    let r = board.search(2, 0);
    let best = r.best_move.expect("depth-2 search must return a move");
    let legal = board.legal_moves();
    assert!(
        legal.iter().any(|m| m.raw().from_sq == best.raw().from_sq
            && m.raw().to_sq == best.raw().to_sq
            && m.raw().promotion == best.raw().promotion),
        "best move must be legal"
    );
    assert_eq!(board.to_tsfen(), before, "search must not mutate the board");
}

#[test]
fn search_expands_a_real_tree() {
    let mut board = Board::initial();
    let r = board.search(3, 0);
    assert!(r.score.abs() < 1_000_000, "non-mate score must be below MATE_SCORE");
    // With the material fast path DISABLED, depth-3 must genuinely expand
    // the tree (the old fake path produced ~1536 nodes, one per root move).
    assert!(r.nodes > 1_000, "depth-3 must expand a real tree (got {} nodes)", r.nodes);
}
