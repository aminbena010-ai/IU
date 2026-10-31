fn main() {
    let url = std::env::args().nth(1);
    browser::start(url);
}
