use std::io::{self, IsTerminal};
use std::net::TcpListener;
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
    /// PostgreSQLのクライアントからの接続を，1つずつ受け付ける
    Serve {
        /// 表を保存するデータディレクトリ．省略するとメモリーに置く
        #[arg(long)]
        data_dir: Option<PathBuf>,
        /// 待ち受けるTCPのポート
        #[arg(long, default_value_t = 5433)]
        port: u16,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Repl { data_dir } => repl(data_dir),
        Command::Serve { data_dir, port } => serve(data_dir, port),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("ferrodb: {message}");
            ExitCode::FAILURE
        }
    }
}

/// データディレクトリのデータベースを開く．ディレクトリがなければメモリーに置く．
fn open(data_dir: Option<PathBuf>) -> Result<Database, String> {
    match data_dir {
        Some(dir) => Database::open(&dir).map_err(|error| error.to_string()),
        None => Ok(Database::new()),
    }
}

fn repl(data_dir: Option<PathBuf>) -> Result<(), String> {
    let database = open(data_dir)?;
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    ferrodb::repl::run_with(database, stdin.lock(), io::stdout().lock(), interactive)
        .map_err(|error| error.to_string())
}

/// 接続を受け付けて，終わるまで扱い，次の接続を待つ．接続のエラーは表示して，次の接続に進む．
fn serve(data_dir: Option<PathBuf>, port: u16) -> Result<(), String> {
    let mut database = open(data_dir)?;
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|error| error.to_string())?;
    println!("ferrodb listening on 127.0.0.1:{port}");
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("ferrodb: {error}");
                continue;
            }
        };
        if let Err(error) = ferrodb::server::connection::handle(stream, &mut database) {
            eprintln!("ferrodb: {error}");
        }
    }
    Ok(())
}
