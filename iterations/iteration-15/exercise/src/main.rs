use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ferrodb::Database;

/// SQLを処理するデータベース
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 標準入力からSQLを読み，結果を標準出力に書く
    Repl {
        /// 表を保存するデータディレクトリ．省略するとメモリーに置く
        #[arg(long)]
        data_dir: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Repl { data_dir } => {
            let database = match data_dir {
                Some(dir) => match Database::open(&dir) {
                    Ok(database) => database,
                    Err(error) => {
                        eprintln!("ferrodb: {error}");
                        return ExitCode::FAILURE;
                    }
                },
                None => Database::new(),
            };
            let stdin = io::stdin();
            let interactive = stdin.is_terminal();
            match ferrodb::repl::run_with(database, stdin.lock(), io::stdout().lock(), interactive)
            {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("ferrodb: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
