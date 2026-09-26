use std::io::{self, IsTerminal};

/// 標準入力からSQLを読み，結果を標準出力に書く．端末から読むときはプロンプトを表示する．
fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    ferrodb::repl::run(stdin.lock(), io::stdout().lock(), interactive)
}
