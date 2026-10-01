const CW: f64 = 8.4;
const CH: f64 = 18.0;
const FONT: f64 = 14.0;

fn hex(color: vt100::Color, fallback: &str) -> String {
    const SYSTEM: [&str; 16] = [
        "#1c1c22", "#e05561", "#8cc265", "#d18f52", "#4aa5f0", "#c162de", "#42b3c2", "#c7c7c7",
        "#6b6b73", "#ff616e", "#a5e075", "#f0a45d", "#4dc4ff", "#de73ff", "#4cd1e0", "#f2f2f2",
    ];
    match color {
        vt100::Color::Default => fallback.to_string(),
        vt100::Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        vt100::Color::Idx(index) => match index {
            0..=15 => SYSTEM[index as usize].to_string(),
            16..=231 => {
                let index = index - 16;
                let level = |value: u8| -> u8 {
                    if value == 0 {
                        0
                    } else {
                        55 + value * 40
                    }
                };
                let (r, g, b) = (level(index / 36), level(index % 36 / 6), level(index % 6));
                format!("#{r:02x}{g:02x}{b:02x}")
            }
            _ => {
                let grey = 8 + (index - 232) * 10;
                format!("#{grey:02x}{grey:02x}{grey:02x}")
            }
        },
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rows: u16 = args.get(3).map_or(26, |value| value.parse().unwrap());
    let columns: u16 = args.get(4).map_or(120, |value| value.parse().unwrap());
    let mut parser = vt100::Parser::new(rows, columns, 0);
    parser.process(&std::fs::read(&args[1]).unwrap());
    let screen = parser.screen();

    let (width, height) = (CW * columns as f64, CH * rows as f64);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" \
         viewBox=\"0 0 {width:.0} {height:.0}\" font-family=\"ui-monospace, SFMono-Regular, \
         Menlo, Consolas, monospace\" font-size=\"{FONT}\">\n\
         <rect width=\"100%\" height=\"100%\" fill=\"#1c1c22\"/>\n"
    );

    for row in 0..rows {
        let mut column = 0;
        while column < columns {
            let Some(cell) = screen.cell(row, column) else {
                column += 1;
                continue;
            };
            let (mut fg, mut bg) = (cell.fgcolor(), cell.bgcolor());
            if cell.inverse() {
                std::mem::swap(&mut fg, &mut bg);
            }
            let (bold, dim, italic) = (cell.bold(), cell.dim(), cell.italic());
            let start = column;
            let mut text = String::new();
            while column < columns {
                let Some(next) = screen.cell(row, column) else {
                    break;
                };
                let (mut nfg, mut nbg) = (next.fgcolor(), next.bgcolor());
                if next.inverse() {
                    std::mem::swap(&mut nfg, &mut nbg);
                }
                if column > start
                    && (nfg != fg
                        || nbg != bg
                        || next.bold() != bold
                        || next.dim() != dim
                        || next.italic() != italic)
                {
                    break;
                }
                let contents = next.contents();
                text.push_str(if contents.is_empty() { " " } else { contents });
                column += 1;
            }
            let cells = (column - start) as f64;
            let (x, y) = (CW * start as f64, CH * row as f64);
            if hex(bg, "#1c1c22") != "#1c1c22" {
                svg += &format!(
                    "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{:.1}\" height=\"{CH}\" fill=\"{}\"/>\n",
                    CW * cells,
                    hex(bg, "#1c1c22")
                );
            }
            if !text.trim().is_empty() {
                svg += &format!(
                    "<text x=\"{x:.1}\" y=\"{:.1}\" fill=\"{}\" textLength=\"{:.1}\" \
                     lengthAdjust=\"spacingAndGlyphs\"{}{}{} xml:space=\"preserve\">{}</text>\n",
                    y + CH - 5.0,
                    hex(fg, "#c7c7c7"),
                    CW * cells,
                    if bold { " font-weight=\"bold\"" } else { "" },
                    if italic { " font-style=\"italic\"" } else { "" },
                    if dim { " opacity=\"0.55\"" } else { "" },
                    escape(&text),
                );
            }
        }
    }
    svg += "</svg>\n";
    std::fs::write(&args[2], svg).unwrap();
}
