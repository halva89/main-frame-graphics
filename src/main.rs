use main_frame::core::engine::Engine;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "assets/main.mff".to_string());
    Engine::run_from(path);
}