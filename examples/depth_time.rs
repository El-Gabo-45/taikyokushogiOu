use taikyokushogi::Board;
fn main() {
    for d in 1..=5u32 {
        let mut b = Board::initial();
        let t = std::time::Instant::now();
        let r = b.search(d, 0);
        println!("d={} nodes={} ms={} best={:?}", d, r.nodes, t.elapsed().as_millis(), r.best_move.map(|m| m.to()));
    }
}
