fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).unwrap();
    let rows = args.get(2).map_or(26, |value| value.parse().unwrap());
    let columns = args.get(3).map_or(120, |value| value.parse().unwrap());
    let mut parser = vt100::Parser::new(rows, columns, 0);
    parser.process(&bytes);
    for (index, line) in parser.screen().contents().lines().enumerate() {
        println!("{index:>2} {line}");
    }
    let (row, column) = parser.screen().cursor_position();
    println!(
        "cursor row={row} column={column} hidden={}",
        parser.screen().hide_cursor()
    );
}
