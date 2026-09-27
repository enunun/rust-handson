# Rustのノート

各Iterationで初めて使うRustの文法と概念を説明する．

| Iteration | 内容 |
| --- | --- |
| [0](iteration-00.md) | Cargo，関数，整数型，`&str`，`enum`と`#[derive]`，`struct`，`Vec`，`Option`と`Result`，`if`と`match`，テスト，モジュール，winnow |
| [1](iteration-01.md) | 代数的データ型(直積型と直和型)，列挙子の形，`match`による分解，再帰的な型と`Box`，参照と`*`，検査付きの整数演算，`?`と`map_err`，`mut`と`for`，`const`，`Eq`，winnowの`TokenSlice`と`expression` |
| [2](iteration-02.md) | `impl`ブロック，メソッドと関連関数，`Option<bool>`による3値，タプルの`match`とorパターン，型による分類，`Ordering`，winnowの`postfix`と`Infix::Neither` |
| [3](iteration-03.md) | 所有権とムーブ，`Clone`と`Copy`，借用の規則，`String`と`&str`，`match`で所有する値を取り出す，UTF-8と文字の数，winnowの`none_of` |
| [4](iteration-04.md) | エラー型の設計，`Display`，`std::error::Error`，`From`と`?`，`PartialEq<B>`，非公開のフィールドとアクセサー，ジェネリックな構造体，`&'static str`と`Copy`，winnowの`LocatingSlice`と`cut_err` |
| [5](iteration-05.md) | `HashMap`，`#[derive(Default)]`，構造体が値を所有する，`Option`で省略を表す，`match`のガードと`matches!`，`enumerate`と`zip`，スライスと`vec!`，`expect` |
| [6](iteration-06.md) | ライブラリクレートとバイナリクレート，`BufRead`と`Write`，`io::Result`，引数の`impl Trait`，書式指定，`IsTerminal`，モジュールの階層 |
| [7](iteration-07.md) | イテレーター，`iter`/`iter_mut`/`into_iter`，クロージャ，`map`/`filter`/`position`/`any`/`collect`，`collect`と`Result`，`ok_or_else` |
| [8](iteration-08.md) | `iter_mut`による書き換え，`retain`，借用の衝突，計算と変更の分離，`if let`，`HashSet`と`Hash`の導出，`contains`と`to_vec` |
| [9](iteration-09.md) | `Ordering`と`then_with`，`Ord`と`PartialOrd`の実装，`sort_by`と安定な並べ替え，`skip`と`take`，`contains`による重複の検査，`unwrap_or_default` |
| [10](iteration-10.md) | トレイトの定義と実装，トレイトオブジェクトと`Box<dyn Trait>`，静的ディスパッチと動的ディスパッチ，`while let`，`let ... else`，イテレーターを構造体に持つ |
| [11](iteration-11.md) | トレイトオブジェクトを組み合わせる構造体，`Option::take`と`replace`，スライスのパターン，`fold`と`reduce`，`self`を受け取るメソッド，`extend_from_slice`と`repeat_n`，`as_ref`，`unreachable!` |
| [12](iteration-12.md) | `HashMap`のキーになる条件と`Hash`と`Eq`の実装，タプル構造体，`entry` API，トレイトオブジェクトを作って返す関数，`ok_or`，使わない引数 |
| [13](iteration-13.md) | 固定長配列とスライス，`Box<[u8; N]>`，`const`，`to_le_bytes`と`from_le_bytes`，`TryFrom`と`TryInto`，`split_at`，ビットの操作，バイト文字列，`Debug`の実装 |
| [14](iteration-14.md) | `std::fs`の関数と`io::ErrorKind`，`File`と`OpenOptions`，`Read`/`Write`/`Seek`と`read_exact`，`&File`での読み書き，`Path`と`PathBuf`，16進数の書式指定，`keys`と`cloned`，`&mut &[u8]`，`drop`，`clap`のderiveと機能(feature)，`ExitCode`，`tempfile`と開発用の依存 |
